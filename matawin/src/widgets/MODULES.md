# Widget module layout

```text
widgets/
├── mod.rs                  # public surface and feature gates
├── controls.rs             # live Button/TextField + shared style handles
├── events.rs               # ids/events/event queue
├── font.rs                 # matafonts renderer
├── geometry.rs             # Rect/anchors/legacy resize policy
├── media.rs                # mata-audio re-exports
├── render.rs               # software control rendering
├── style/
│   ├── mod.rs              # style exports
│   ├── length.rs           # px/%/vw/vh/vmin/vmax/em/rem
│   ├── box_model.rs        # margin/padding/border-radius
│   ├── position.rs         # relative/absolute/z-index
│   ├── typography.rs       # font properties
│   ├── pseudo.rs           # interactive/structural pseudo-classes
│   ├── media.rs            # media queries
│   ├── animation.rs        # keyframes/timing/iterations
│   └── state.rs            # Style/StylePatch/rules/computed layout
├── window.rs               # host, input, z-order, style resolution
├── webview.rs              # optional public WebView wrapper (`webview` feature)
├── linux.rs                # optional Linux WebKitGTK FFI backend
├── windows/                # optional WebView2 backend
│   ├── mod.rs
│   ├── callbacks.rs
│   ├── loader.rs
│   └── backend.rs
└── catalog/
    ├── mod.rs
    ├── base.rs             # common Widget/WidgetExt/ContainerState
    ├── buttons.rs
    ├── containers.rs
    ├── drawing.rs
    ├── image.rs
    ├── lists.rs
    ├── media_widgets.rs
    ├── misc.rs
    ├── text.rs
    ├── types.rs
    └── values.rs
```
