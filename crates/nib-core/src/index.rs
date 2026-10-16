//! Incremental lookup tables, reverse dependencies and constant-time backlink removal.
use crate::{
    LinkKind, LinkTarget, NoteId, NoteMeta, Tag,
    links::{fragments, resolve, resolve_markdown},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
/// Metadata index owned and patched by the GTK main context.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// Every indexed note, without body text.
    pub notes: HashMap<NoteId, NoteMeta>,
    /// Vault-relative attachment paths from the walk.
    pub attachments: Vec<String>,
    /// Lowercase stem candidates.
    pub by_stem: HashMap<String, BTreeSet<NoteId>>,
    /// Lowercase alias candidates.
    pub by_alias: HashMap<String, BTreeSet<NoteId>>,
    /// Preferred case-insensitive path match (exact matches use notes directly).
    pub by_path: HashMap<String, NoteId>,
    /// Target to source and one-based line. Order is unspecified; the UI groups/sorts.
    pub backlinks: HashMap<NoteId, Vec<(NoteId, u32)>>,
    /// Folded tags and their source notes.
    pub tags: BTreeMap<Tag, BTreeSet<NoteId>>,
    /// Unresolved written destinations and source notes.
    pub unresolved: BTreeMap<String, BTreeSet<NoteId>>,
    path_variants: HashMap<String, BTreeSet<NoteId>>,
    back_positions: HashMap<NoteId, HashMap<(NoteId, u32), usize>>,
    dependents: HashMap<String, BTreeSet<NoteId>>,
}
impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        self.notes == other.notes
            && self.attachments == other.attachments
            && self.by_stem == other.by_stem
            && self.by_alias == other.by_alias
            && self.by_path == other.by_path
            && self.tags == other.tags
            && self.unresolved == other.unresolved
            && self.dependents == other.dependents
            && self.backlinks.len() == other.backlinks.len()
            && self.backlinks.iter().all(|(id, rows)| {
                other.backlinks.get(id).is_some_and(|v| {
                    rows.iter().collect::<BTreeSet<_>>() == v.iter().collect::<BTreeSet<_>>()
                })
            })
    }
}
impl Eq for Index {}
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
    let joined = if (kind == LinkKind::Markdown || p.starts_with("./") || p.starts_with("../"))
        && !crate::links::has_scheme(&p)
    {
        if p.starts_with('/') || from.folder().is_empty() {
            p.trim_start_matches('/').into()
        } else {
            format!("{}/{p}", from.folder())
        }
    } else {
        p
    };
    NoteId::new(&joined).map_or_else(|_| joined.to_lowercase(), |id| id.as_str().to_lowercase())
}
fn remove_lookup(map: &mut HashMap<String, BTreeSet<NoteId>>, key: &str, id: &NoteId) {
    if let Some(v) = map.get_mut(key) {
        v.remove(id);
        if v.is_empty() {
            map.remove(key);
        }
    }
}
fn remove_backlink(
    rows: &mut HashMap<NoteId, Vec<(NoteId, u32)>>,
    positions: &mut HashMap<NoteId, HashMap<(NoteId, u32), usize>>,
    target: &NoteId,
    source: &NoteId,
    line: u32,
) {
    let Some(slots) = positions.get_mut(target) else {
        return;
    };
    let Some(slot) = slots.remove(&(source.clone(), line)) else {
        return;
    };
    let Some(values) = rows.get_mut(target) else {
        return;
    };
    values.swap_remove(slot);
    if let Some(moved) = values.get(slot) {
        slots.insert(moved.clone(), slot);
    }
    if values.is_empty() {
        rows.remove(target);
        positions.remove(target);
    }
}
impl Index {
    /// Build lookups first, then resolve every outgoing link once.
    pub fn build(metas: Vec<NoteMeta>) -> Self {
        Self::build_with_attachments(metas, Vec::new())
    }
    /// Build an initial index with attachments available during resolution.
    pub fn build_with_attachments(metas: Vec<NoteMeta>, mut attachments: Vec<String>) -> Self {
        attachments.sort();
        attachments.dedup();
        let mut index = Self {
            attachments,
            ..Self::default()
        };
        for m in metas {
            if let Some(old) = index.notes.remove(&m.id) {
                index.remove_lookup(&old);
            }
            index.add_lookup(&m);
            index.notes.insert(m.id.clone(), m);
        }
        let mut ids: Vec<_> = index.notes.keys().cloned().collect();
        ids.sort();
        for id in ids {
            index.attach(&id);
        }
        index
    }
    fn add_lookup(&mut self, m: &NoteMeta) {
        self.by_stem
            .entry(m.id.stem().to_lowercase())
            .or_default()
            .insert(m.id.clone());
        for alias in &m.aliases {
            self.by_alias
                .entry(alias.to_lowercase())
                .or_default()
                .insert(m.id.clone());
        }
        let key = m.id.as_str().to_lowercase();
        let variants = self.path_variants.entry(key.clone()).or_default();
        variants.insert(m.id.clone());
        if let Some(first) = variants.first() {
            self.by_path.insert(key, first.clone());
        }
    }
    fn remove_lookup(&mut self, m: &NoteMeta) {
        remove_lookup(&mut self.by_stem, &m.id.stem().to_lowercase(), &m.id);
        for alias in &m.aliases {
            remove_lookup(&mut self.by_alias, &alias.to_lowercase(), &m.id);
        }
        let key = m.id.as_str().to_lowercase();
        remove_lookup(&mut self.path_variants, &key, &m.id);
        if let Some(first) = self.path_variants.get(&key).and_then(BTreeSet::first) {
            self.by_path.insert(key, first.clone());
        } else {
            self.by_path.remove(&key);
        }
    }
    fn detach(&mut self, id: &NoteId) {
        let Some(m) = self.notes.get(id) else {
            return;
        };
        for tag in &m.tags {
            if let Some(v) = self.tags.get_mut(tag) {
                v.remove(id);
                if v.is_empty() {
                    self.tags.remove(tag);
                }
            }
        }
        for link in &m.links {
            let key = link_key(id, &link.raw, link.kind);
            if let Some(v) = self.dependents.get_mut(&key) {
                v.remove(id);
                if v.is_empty() {
                    self.dependents.remove(&key);
                }
            }
            match &link.target {
                LinkTarget::Note { id: target, .. } => remove_backlink(
                    &mut self.backlinks,
                    &mut self.back_positions,
                    target,
                    id,
                    link.line,
                ),
                LinkTarget::Unresolved(raw) => {
                    if let Some(v) = self.unresolved.get_mut(raw) {
                        v.remove(id);
                        if v.is_empty() {
                            self.unresolved.remove(raw);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn attach(&mut self, id: &NoteId) {
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
        for tag in &m.tags {
            self.tags.entry(tag.clone()).or_default().insert(id.clone());
        }
        for (link, target) in m.links.iter_mut().zip(targets) {
            link.target = target;
            self.dependents
                .entry(link_key(id, &link.raw, link.kind))
                .or_default()
                .insert(id.clone());
            match &link.target {
                LinkTarget::Note { id: target, .. } => {
                    let key = (id.clone(), link.line);
                    let positions = self.back_positions.entry(target.clone()).or_default();
                    if !positions.contains_key(&key) {
                        let rows = self.backlinks.entry(target.clone()).or_default();
                        positions.insert(key.clone(), rows.len());
                        rows.push(key);
                    }
                }
                LinkTarget::Unresolved(raw) => {
                    self.unresolved
                        .entry(raw.clone())
                        .or_default()
                        .insert(id.clone());
                }
                _ => {}
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
    /// Patch one note and only sources dependent on its path, stem or aliases.
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
            self.attach(&id);
        }
    }
    /// Remove a note and invalidate only sources whose resolution can change.
    pub fn remove(&mut self, id: &NoteId) {
        let Some(meta) = self.notes.get(id) else {
            return;
        };
        let mut affected = self.affected(&keys(meta));
        affected.insert(id.clone());
        for source in &affected {
            self.detach(source);
        }
        if let Some(old) = self.notes.remove(id) {
            self.remove_lookup(&old);
        }
        affected.remove(id);
        for source in affected {
            self.attach(&source);
        }
    }
    /// Change an identity; apply approved edits and watcher reparses afterwards.
    pub fn rename(&mut self, old: &NoteId, new: NoteId) {
        if let Some(mut meta) = self.notes.get(old).cloned() {
            self.remove(old);
            meta.id = new;
            self.upsert(meta);
        }
    }
    /// Add an attachment and revisit only links using its path or basename.
    pub fn upsert_attachment(&mut self, path: &str) {
        if let Err(i) = self.attachments.binary_search_by(|p| p.as_str().cmp(path)) {
            self.attachments.insert(i, path.into());
        }
        self.refresh_attachment(path);
    }
    /// Remove an attachment without rebuilding unrelated notes.
    pub fn remove_attachment(&mut self, path: &str) {
        if let Ok(i) = self.attachments.binary_search_by(|p| p.as_str().cmp(path)) {
            self.attachments.remove(i);
        }
        self.refresh_attachment(path);
    }
    fn refresh_attachment(&mut self, path: &str) {
        let sources = self.affected(&[
            path.to_lowercase(),
            path.rsplit('/').next().unwrap_or(path).to_lowercase(),
        ]);
        for id in &sources {
            self.detach(id);
        }
        for id in sources {
            self.attach(&id);
        }
    }
    /// Borrow backlinks without allocating; consumers choose their display order.
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
