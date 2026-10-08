# Dependencies

`mata-video` has no unconditional Rust dependencies.

## Windows

Direct Rust dependency:

- `windows = 0.62.2`

Used only for Win32 COM and Media Foundation APIs.

## Linux

Direct Rust dependency:

- `gstreamer = 0.25.4`

No direct `gstreamer-app` or `glib` dependency is used. The GStreamer crate
re-exports the GLib functionality required to turn a filesystem path into a
URI, while `appsink` is controlled through dynamic GObject action signals.
