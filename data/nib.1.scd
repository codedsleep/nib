nib(1)

# NAME

nib - keyboard-first Markdown notes for an Obsidian vault

# SYNOPSIS

*nib* [*--profile* _NAME_] [*--vault* _DIR_] [_PATH_]

*nib* [*--profile* _NAME_] [*--vault* _DIR_] *--new* _TITLE_

*nib* [*--profile* _NAME_] [*--vault* _DIR_] *--daily*

*nib* *--version* | *--help*

# DESCRIPTION

nib edits local Markdown notes using GTK 4 and GtkSourceView 5. It supports
wikilinks, backlinks, fuzzy finding, vault search, daily notes, templates,
attachments and a reading view. No plugin system or network service is used.

With no arguments nib opens the last vault, or asks for a folder on first run.
_PATH_ may name a vault, a subfolder, or a Markdown note. For a note nib finds
the nearest ancestor containing .obsidian or .nib, falling back to its folder.
Subsequent invocations forward to the running instance of that profile.
A second vault needs a separate profile. --new creates a note without replacing
an existing note. --daily opens or creates today's daily note.

# OPTIONS

*--vault* _DIR_
	Use this vault instead of the last one.

*--profile* _NAME_
	Use independent config, session and application instance. Names contain
	letters, digits or underscores.

*--new* _TITLE_
	Create and open a note in the configured new-note folder.

*--daily*
	Open or create today's daily note.

*--version*, *--help*
	Print the version or usage and exit.

# KEYS AND COMMANDS

Generated from crates/nib/src/registry.rs. F1 opens this same registry in the
application. Alt+P opens the palette; features remain available with vim off.
Files-column sequences apply only while that column has keyboard focus.

## WINDOW AND EDITOR

*Ctrl+S, :w, :w!, :write*
	Save note

*Ctrl+Q, :qa, :qa!, :qall*
	Quit

*Alt+T, :tabnew*
	New tab

*Alt+W*
	Close the tab

*Alt+[*
	Previous tab

*Alt+]*
	Next tab

*Alt+Left*
	Back to the tab's previous note

*Alt+Right*
	Forward to the tab's next note

*Alt+D, :daily*
	Today's daily note

*Alt+N, :new*
	New note

*Alt+Shift+T, :template*
	Insert a template

*Alt+O, :o, :open*
	Open a note (quick switcher)

*Alt+P*
	Command palette

*Alt+F, :find*
	Search the vault

*Alt+Enter, :follow*
	Follow the link under the cursor

*Ctrl+Enter, :check*
	Toggle the task checkbox

*Ctrl+Space*
	Complete the link or tag at the cursor

*Ctrl+Shift+Up*
	Promote the heading

*Ctrl+Shift+Down*
	Demote the heading

*Alt+E, :read*
	Reading view (toggle)

*Alt+Shift+E, :split*
	Source and reading side by side

*F9, :files*
	Show or hide the files column

*F8*
	Show or hide the right panel

*F7*
	Show or hide the parent column

*Alt+B, :backlinks*
	Focus the backlinks

*F11*
	Fullscreen

*F5*
	Reload the folder listing from disk

*F1, :help*
	List every key and command

*Alt+1*
	Focus the files column

*Alt+2*
	Focus the editor

*Alt+3*
	Focus the right panel

## FILES COLUMN

*j, <Down>*
	Cursor down

*k, <Up>*
	Cursor up

*gg, <Home>*
	First entry

*G, <End>*
	Last entry

*<C-d>*
	Half a page down

*<C-u>*
	Half a page up

*<C-f>, <PageDown>*
	A page down

*<C-b>, <PageUp>*
	A page up

*h, <Left>, <BS>*
	Parent folder

*l, <CR>, o, <Right>*
	Enter the folder or open the file

*O*
	Open in a new tab

*H*
	Previous folder

*L*
	Next folder

*<Space>*
	Select or unselect, then step down

*v*
	Visual selection mode

*<C-a>*
	Select everything

*<C-r>*
	Invert the selection

*<Esc>*
	Clear the selection and the filter

*y*
	Yank (copy) to the clipboard

*x*
	Cut to the clipboard

*p*
	Paste here

*P*
	Paste here, replacing existing names

*Y*
	Forget the yank

*d*
	Move to the trash

*a*
	Create a note (name/ for a folder)

*r*
	Rename (rewriting links)

*/*
	Find in the list

*?*
	Find backwards

*n*
	Next match

*N*
	Previous match

*f*
	Filter the list

*z*
	Fuzzy-find below this folder

*.*
	Show or hide hidden files

*,n*
	Sort by name

*,m*
	Sort by modified time, showing ages

*,r*
	Reverse the order

*cp*
	Copy the absolute path

*cn*
	Copy the name

*cl*
	Copy a [[wikilink]]

*cr*
	Copy the vault-relative path

*gh*
	Go to the vault root

*gd*
	Go to the daily folder

*ga*
	Go to the attachments folder

*gt*
	Go to the templates folder

*gr*
	Recent notes

*~*
	List every key and command

*<Tab>*
	Focus the editor

## COMMANDS AND PALETTE

*:wq, :x*
	Save and close the tab

*:q, :quit*
	Close the tab

*:q!*
	Close the tab, dropping changes

*:e*
	Open a note by name

*:e!*
	Reload the note from disk, dropping changes

*:rename*
	Rename (rewriting links)

*:trash*
	Move to the trash

*:tab*
	Tab N, new, close, next or prev

*:goto*
	Go to a line

*:paste-image*
	Paste an image as an attachment

*:attach*
	Attach files

*:config*
	Edit nib.toml

*:reindex*
	Index the vault again

*:tag*
	Notes by tag

*:edit*
	Back to the source (:edit NOTE opens one)

*:outline*
	Focus the outline

*:version*
	Show nib's version

# EDITING

GtkSourceView supplies vim motions, modes, registers, undo, search and text
objects. nib does not implement :map or gf. Substitution support depends on
GtkSourceView and is not reliable in the tested 5.20 release.

Enter continues lists in insert mode; Tab and Shift+Tab indent list items.
Ctrl+V pastes a clipboard image only in insert mode (or with vim off) and when
the clipboard offers a texture. :paste-image works in every mode. Finder
Shift+Enter opens in a new tab. Reading keys: j k gg G Ctrl+D Ctrl+U, Tab to
focus links, Enter to follow, i to return to source.

:daily accepts a signed day offset or YYYY-MM-DD. :tab accepts a 1-based number,
new, close, next or prev. :w NAME names an untitled note. :e! drops edits and
reloads. A modified note changed externally is never autosaved over; :w is an
explicit overwrite. Trash uses XDG trash, with no permanent-delete fallback.

# FILES

_$XDG\_CONFIG\_HOME/nib/nib.toml_
	Configuration, edited with :config. Defaults to ~/.config/nib/nib.toml.
	Profiles add NAME before nib.toml. Editor changes apply live on save;
	vault folder settings apply on the next vault open.

_$XDG\_STATE\_HOME/nib/VAULT\_KEY/session.toml_
	Tabs, cursor positions, panes and files-column state. Defaults to
	~/.local/state/nib. Profiles add NAME before VAULT\_KEY.

_$XDG\_STATE\_HOME/nib/crash.log_
	Crash locations, backtraces, versions and vault key; no note contents.

_~/.local/share/gtksourceview-5/styles/nib-\*.xml_
	Installed editor and reading-view colour schemes.

_~/.local/share/gtksourceview-5/language-specs/nib-markdown.lang_
	Markdown that highlights fenced code in the language its fence names.
	Loaded the first time a note names one; other GtkSourceView applications
	do not list it.

# ENVIRONMENT

*NIB\_LOG*
	Logging level: debug, info, warn (default), error or off.

*NIB\_TRACE=1*
	Print performance measurements to standard error.

*NIB\_ALLOW\_ROOTS*
	When set, confine writes to colon-separated roots plus the fixture cache
	and temporary directory. An empty value still enables confinement.
	A leading ~ is expanded. Unset for normal installed use.

*NIB\_GTK\_THEME*
	Override the desktop theme name; GTK 3-only themes supply a palette.

*GTK\_THEME*
	Explicit GTK theme selection, for example Adwaita:light.

# SEE ALSO

*gh*(1), *gtk4-launch*(1)
