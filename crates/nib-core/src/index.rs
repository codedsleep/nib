//! Incremental lookup tables and reverse dependencies without a full-vault scan.
use crate::{
    LinkKind, LinkTarget, NoteId, NoteMeta, Tag,
    links::{fragments, resolve, resolve_markdown},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
/// Metadata index owned and patched by the GTK main context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Every indexed note, without body text.
    pub notes: HashMap<NoteId, NoteMeta>,
    /// Vault-relative attachment paths from the walk, never decoded as notes.
    pub attachments: Vec<String>,
    /// Lowercase stem candidates.
    pub by_stem: HashMap<String, Vec<NoteId>>,
    /// Lowercase alias candidates.
    pub by_alias: HashMap<String, Vec<NoteId>>,
    /// Case-insensitive path lookup.
    pub by_path: HashMap<String, NoteId>,
    /// Target to source and one-based line.
    pub backlinks: HashMap<NoteId, Vec<(NoteId, u32)>>,
    /// Folded tags and their source notes.
    pub tags: BTreeMap<Tag, Vec<NoteId>>,
    /// Unresolved written destinations and source notes.
    pub unresolved: BTreeMap<String, Vec<NoteId>>,
    dependents: HashMap<String, BTreeSet<NoteId>>,
}
fn keys(meta: &NoteMeta) -> Vec<String> {
    let mut keys = vec![
        meta.id.as_str().to_lowercase(),
        meta.id.stem().to_lowercase(),
    ];
    keys.extend(meta.aliases.iter().map(|a| a.to_lowercase()));
    keys.sort();
    keys.dedup();
    keys
}
fn link_key(from: &NoteId, raw: &str, kind: LinkKind) -> String {
    let (p, _, _) = fragments(raw);
    let p = crate::links::decode_path(p);
    if kind == LinkKind::Markdown && !crate::links::has_scheme(&p) {
        let joined = if p.starts_with('/') || from.folder().is_empty() {
            p.trim_start_matches('/').to_owned()
        } else {
            format!("{}/{p}", from.folder())
        };
        NoteId::new(&joined)
            .map_or(joined, |id| id.to_string())
            .to_lowercase()
    } else {
        p.trim_end_matches(".md").to_lowercase()
    }
}
fn add(map: &mut HashMap<String, Vec<NoteId>>, key: String, id: &NoteId) {
    let v = map.entry(key).or_default();
    if !v.contains(id) {
        v.push(id.clone());
        v.sort();
    }
}
fn remove(map: &mut HashMap<String, Vec<NoteId>>, key: &str, id: &NoteId) {
    if let Some(v) = map.get_mut(key) {
        v.retain(|n| n != id);
        if v.is_empty() {
            map.remove(key);
        }
    }
}
impl Index {
    /// Build lookups first, then resolve every outgoing link once.
    pub fn build(metas: Vec<NoteMeta>) -> Self {
        let mut index = Self::default();
        for m in metas {
            index.add_lookup(&m);
            index.notes.insert(m.id.clone(), m);
        }
        let ids: Vec<_> = index.notes.keys().cloned().collect();
        for id in ids {
            index.attach(&id, true);
        }
        for ids in index.tags.values_mut() {
            ids.sort();
            ids.dedup();
        }
        for ids in index.unresolved.values_mut() {
            ids.sort();
            ids.dedup();
        }
        for refs in index.backlinks.values_mut() {
            refs.sort();
            refs.dedup();
        }
        index
    }
    fn add_lookup(&mut self, m: &NoteMeta) {
        add(&mut self.by_stem, m.id.stem().to_lowercase(), &m.id);
        for a in &m.aliases {
            add(&mut self.by_alias, a.to_lowercase(), &m.id);
        }
        self.by_path
            .entry(m.id.as_str().to_lowercase())
            .and_modify(|id| {
                if m.id < *id {
                    *id = m.id.clone();
                }
            })
            .or_insert_with(|| m.id.clone());
    }
    fn remove_lookup(&mut self, m: &NoteMeta) {
        remove(&mut self.by_stem, &m.id.stem().to_lowercase(), &m.id);
        for a in &m.aliases {
            remove(&mut self.by_alias, &a.to_lowercase(), &m.id);
        }
        self.by_path.remove(&m.id.as_str().to_lowercase());
    }
    fn detach(&mut self, id: &NoteId) {
        let Some(m) = self.notes.get(id) else {
            return;
        };
        for t in &m.tags {
            if let Some(v) = self.tags.get_mut(t) {
                v.retain(|n| n != id);
                if v.is_empty() {
                    self.tags.remove(t);
                }
            }
        }
        for l in &m.links {
            let key = link_key(id, &l.raw, l.kind);
            if let Some(v) = self.dependents.get_mut(&key) {
                v.remove(id);
                if v.is_empty() {
                    self.dependents.remove(&key);
                }
            }
            match &l.target {
                LinkTarget::Note { id: target, .. } => {
                    if let Some(v) = self.backlinks.get_mut(target) {
                        v.retain(|(n, _)| n != id);
                        if v.is_empty() {
                            self.backlinks.remove(target);
                        }
                    }
                }
                LinkTarget::Unresolved(raw) => {
                    if let Some(v) = self.unresolved.get_mut(raw) {
                        v.retain(|n| n != id);
                        if v.is_empty() {
                            self.unresolved.remove(raw);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn attach(&mut self, id: &NoteId, bulk: bool) {
        let targets: Vec<_> = self
            .notes
            .get(id)
            .into_iter()
            .flat_map(|m| &m.links)
            .map(|l| {
                if l.kind == LinkKind::Wiki {
                    resolve(self, id, &l.raw)
                } else {
                    resolve_markdown(self, id, &l.raw)
                }
            })
            .collect();
        let Some(m) = self.notes.get_mut(id) else {
            return;
        };
        for t in &m.tags {
            let v = self.tags.entry(t.clone()).or_default();
            if bulk {
                v.push(id.clone());
            } else if let Err(i) = v.binary_search(id) {
                v.insert(i, id.clone());
            }
        }
        let mut changed_targets = BTreeSet::new();
        for (l, target) in m.links.iter_mut().zip(targets) {
            l.target = target;
            self.dependents
                .entry(link_key(id, &l.raw, l.kind))
                .or_default()
                .insert(id.clone());
            match &l.target {
                LinkTarget::Note { id: target, .. } => {
                    let v = self.backlinks.entry(target.clone()).or_default();
                    v.push((id.clone(), l.line));
                    changed_targets.insert(target.clone());
                }
                LinkTarget::Unresolved(raw) => {
                    let v = self.unresolved.entry(raw.clone()).or_default();
                    if bulk {
                        v.push(id.clone());
                    } else if let Err(i) = v.binary_search(id) {
                        v.insert(i, id.clone());
                    }
                }
                _ => {}
            }
        }
        for target in changed_targets.into_iter().filter(|_| !bulk) {
            if let Some(v) = self.backlinks.get_mut(&target) {
                v.sort();
                v.dedup();
            }
        }
    }
    fn affected(&self, keys: &[String]) -> BTreeSet<NoteId> {
        keys.iter()
            .filter_map(|k| self.dependents.get(k))
            .flatten()
            .cloned()
            .collect()
    }
    /// Replace one note and revisit only links dependent on its path, stem or aliases.
    pub fn upsert(&mut self, meta: NoteMeta) {
        let mut k = keys(&meta);
        if let Some(old) = self.notes.get(&meta.id) {
            k.extend(keys(old));
        }
        let mut affected = self.affected(&k);
        affected.insert(meta.id.clone());
        for id in &affected {
            self.detach(id);
        }
        if let Some(old) = self.notes.remove(&meta.id) {
            self.remove_lookup(&old);
        }
        self.add_lookup(&meta);
        self.notes.insert(meta.id.clone(), meta);
        for id in affected {
            self.attach(&id, false);
        }
    }
    /// Remove one note and invalidate only sources whose resolution can change.
    pub fn remove(&mut self, id: &NoteId) {
        let Some(m) = self.notes.get(id) else {
            return;
        };
        let mut affected = self.affected(&keys(m));
        affected.insert(id.clone());
        for a in &affected {
            self.detach(a);
        }
        if let Some(old) = self.notes.remove(id) {
            self.remove_lookup(&old);
        }
        affected.remove(id);
        for a in affected {
            self.attach(&a, false);
        }
    }
    /// Change an identity; use watcher reparses after applying a rename plan.
    pub fn rename(&mut self, old: &NoteId, new: NoteId) {
        if let Some(mut m) = self.notes.get(old).cloned() {
            self.remove(old);
            m.id = new;
            self.upsert(m);
        }
    }
    /// Borrow backlinks without allocating.
    pub fn backlinks(&self, id: &NoteId) -> &[(NoteId, u32)] {
        self.backlinks.get(id).map_or(&[], Vec::as_slice)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_note;
    use std::time::SystemTime;
    fn m(id: &str, text: &str) -> NoteMeta {
        parse_note(NoteId::new(id).unwrap(), text, SystemTime::UNIX_EPOCH, 0)
    }
    #[test]
    fn incremental() {
        let mut i = Index::build(vec![m("a", "[[Foo]] #tag")]);
        let before = i.clone();
        i.upsert(m("Foo", "# Foo"));
        assert_eq!(i.backlinks(&NoteId::new("Foo").unwrap()).len(), 1);
        i.remove(&NoteId::new("Foo").unwrap());
        assert_eq!(i, before);
        i.upsert(m("a", "[[Other]] #new"));
        assert!(!i.tags.contains_key(&Tag::new("tag")));
        assert!(!i.unresolved.contains_key("Foo"));
    }
    #[test]
    fn resolution() {
        let i = Index::build(vec![
            m("x/Same", ""),
            m("a/Same", ""),
            m("Same", ""),
            m("alias", "---\naliases: [other]\n---\n"),
        ]);
        let f = NoteId::new("x/source").unwrap();
        for (raw, id) in [
            ("same", "x/Same"),
            ("a/same.md", "a/Same"),
            ("OTHER", "alias"),
            ("Same#Missing", "x/Same"),
        ] {
            assert!(matches!(resolve(&i,&f,raw),LinkTarget::Note{id:n,..}if n.as_str()==id));
        }
        assert!(
            matches!(resolve(&i,&NoteId::new("z").unwrap(),"Same"),LinkTarget::Note{id,..}if id.as_str()=="Same")
        );
    }
}
