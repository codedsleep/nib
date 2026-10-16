//! Files on the compositor clipboard (P§8), in the formats trek and the
//! GNOME-family file managers use: `x-special/gnome-copied-files`,
//! `text/uri-list`, a `GdkFileList` and plain text. The encoders are
//! `nib-core::clip`; this module only moves their bytes.
use crate::gtk;
use gtk::{gdk, prelude::*};
use std::path::PathBuf;

pub const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
pub const URI_LIST: &str = "text/uri-list";

fn clipboard() -> Option<gdk::Clipboard> {
    gdk::Display::default().map(|d| d.clipboard())
}

fn bytes(text: String) -> glib::Bytes {
    glib::Bytes::from_owned(text.into_bytes())
}

/// `y` / `x`: one union provider offering every format.
pub fn set_files(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    let clipboard = clipboard().ok_or("no display")?;
    let strings: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let gnome =
        nib_core::clip::encode_gnome_copied(&strings, cut).map_err(|_| "not a local path")?;
    let uris = nib_core::clip::encode_uri_list(&strings).map_err(|_| "not a local path")?;
    let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
    let providers = [
        gdk::ContentProvider::for_bytes(GNOME_COPIED_FILES, &bytes(gnome)),
        gdk::ContentProvider::for_bytes(URI_LIST, &bytes(uris)),
        gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value()),
        gdk::ContentProvider::for_value(&strings.join("\n").to_value()),
    ];
    clipboard
        .set_content(Some(&gdk::ContentProvider::new_union(&providers)))
        .map_err(|e| e.to_string())
}

/// `cl`, `cp`, `cn`, `cr`: plain text.
pub fn set_text(text: &str) {
    if let Some(clipboard) = clipboard() {
        clipboard.set_text(text);
    }
}

/// `Y` and after a cut was pasted: empty the clipboard if nib owns it.
pub fn clear_own() {
    if let Some(clipboard) = clipboard()
        && clipboard.is_local()
        && let Err(e) = clipboard.set_content(gdk::ContentProvider::NONE)
    {
        log::warn!("clipboard: {e}");
    }
}

async fn read_text(clipboard: &gdk::Clipboard, mime: &str) -> Option<String> {
    let (stream, _) = clipboard
        .read_future(&[mime], glib::Priority::DEFAULT)
        .await
        .ok()?;
    let buffer = gio::MemoryOutputStream::new_resizable();
    buffer
        .splice_future(
            &stream,
            gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
            glib::Priority::DEFAULT,
        )
        .await
        .ok()?;
    let text = String::from_utf8(buffer.steal_as_bytes().to_vec()).ok()?;
    Some(text.trim_end_matches('\0').to_owned())
}

/// Files on the clipboard and whether they were cut, or None.
pub async fn read_files() -> Option<(Vec<PathBuf>, bool)> {
    let clipboard = clipboard()?;
    let formats = clipboard.formats();
    // The GNOME format says cut or copy, so it is preferred.
    let (paths, cut) = if formats.contain_mime_type(GNOME_COPIED_FILES) {
        let text = read_text(&clipboard, GNOME_COPIED_FILES).await?;
        nib_core::clip::decode_gnome_copied(&text).ok()?
    } else if formats.contain_mime_type(URI_LIST) {
        let text = read_text(&clipboard, URI_LIST).await?;
        (nib_core::clip::decode_uri_list(&text).ok()?, false)
    } else if formats.contains_type(gdk::FileList::static_type()) {
        let value = clipboard
            .read_value_future(gdk::FileList::static_type(), glib::Priority::DEFAULT)
            .await
            .ok()?;
        let list = value.get::<gdk::FileList>().ok()?;
        let paths = list
            .files()
            .iter()
            .filter_map(gio::File::path)
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        (paths, false)
    } else {
        return None;
    };
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    (!paths.is_empty()).then_some((paths, cut))
}

/// A texture on the clipboard (a screenshot), for `Ctrl+V` / `:paste-image`.
pub async fn read_texture() -> Option<gdk::Texture> {
    let clipboard = clipboard()?;
    if !clipboard
        .formats()
        .contains_type(gdk::Texture::static_type())
    {
        return None;
    }
    clipboard.read_texture_future().await.ok().flatten()
}

/// Whether the clipboard offers a texture right now.
pub fn has_texture() -> bool {
    clipboard().is_some_and(|c| offers_texture(&c))
}

/// Another program's offer lists MIME types; GDK can turn `image/png` and
/// friends into a texture.
fn offers_texture(clipboard: &gdk::Clipboard) -> bool {
    clipboard
        .formats()
        .union_deserialize_types()
        .contains_type(gdk::Texture::static_type())
}
