# Apps and processes (fork additions)

What the fork adds to upstream's [Apps](../../src/apps.md) and
[Commands and Files](../../src/processes.md) pages: more of a desktop entry (id, categories,
keywords, actions, the files it opens), opening files with an app, an icon lookup, launching with
an activation token, and `watch_file` seeing deletes.

## Usage

### `DesktopApp`

New functions, next to upstream's `name`, `description`, `exec`, `icon`, `icon_path` and `launch`:

| Function | Gives or does |
|---|---|
| `id()` | its desktop file's id, like `"org.mozilla.firefox.desktop"`: stays the same while it's installed, a key to remember it by |
| `categories()` | its freedesktop `Categories`, like `["Network", "WebBrowser"]` |
| `keywords()` | its `Keywords`, extra search terms the author added |
| `actions()` | its desktop actions, other ways to start it like "New Private Window" (see below) |
| `mime_types()` | the kinds of file it says it opens (its `MimeType`), like `["text/markdown", "text/plain"]` |
| `opens(file)` | whether it opens `file`, by the kind of file its name says it is |
| `commands_with(files)` | the command lines that open `files` with it (see below) |
| `launch_with(files)` | starts it on `files`, without waiting |

Each `DesktopAction` is one of the `[Desktop Action …]` sections the entry's `Actions` lists, in
that order; one without a `Name` or `Exec` is left out. It has `name()`, `exec()` (without the
placeholders), `icon()` (its own icon's name, or `None` to use the app's) and `launch()`:

```rust,ignore
for action in app.actions() {
    println!("{}: {}", action.name(), action.exec());
}
```

### Opening files

`opens(file)` finds the file's kind from its name in the shared MIME database (`mime/globs2` in the XDG data dirs; a folder is `inode/directory`), and says yes when the app lists that kind or one it comes from (`mime/subclasses`; every `text/…` kind is `text/plain` too). An app with no `MimeType` opens nothing.

`commands_with(files)` fills the files into the entry's `Exec`: `%F` and `%U` take them all, as paths and as `file://` links, while `%f` and `%u` take one, so there's a command per file. Every file is quoted for the shell. An `Exec` with none of them opens no files, and you get the plain command. `launch_with(files)` runs them. To have the compositor run them instead, hand each to `Windows::spawn(vec!["sh".into(), "-c".into(), command])`.

```rust,ignore
let file = PathBuf::from("/home/me/notes.md");

if app.opens(&file) {
    app.launch_with(&[file]);
}
```

### Icons without a desktop file

`Apps::find_icon(name)` looks up an icon by its name in the icon theme (or takes a full path as it is) and gives its image file, or `None`. Use it for something with no `.desktop` file, like a window whose app_id names an icon:

```rust,ignore
let icon = Apps::find_icon("org.gnome.Nautilus");
```

### Launching apps

| Function | Does | Waits? |
|---|---|---|
| `amane::spawn_activated(command)` | like `spawn`, letting the program take the focus | no |

A launcher, a dock or a button that opens an app should use `spawn_activated` instead of `spawn`,
and call it from input:

```rust,ignore
.on_click(|_| amane::spawn_activated("firefox"))
```

`DesktopApp::launch` and `launch_with` launch this way. How the token works, and how an amane app
uses one, is in [Activation](../windows/activation.md).

### Watching files

`watch_file` gives an item each time the file is written, created, replaced, deleted or moved away (upstream missed the last two, `7c36e11`).

## Design / internals

- The MIME lookup is `src/services/apps/mime.rs`, read from the shared-mime-info files.
- Commits: `0ebcfd2` (categories, keywords), `d9d3d46` (actions), `7f48e10` (`Apps::find_icon`), `c029184` (MIME, `opens`, `launch_with`), `3c3d56b` (`id`), `82337f2` (`spawn_activated`).
