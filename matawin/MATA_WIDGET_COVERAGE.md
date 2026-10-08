# Mata widget catalog coverage

`matawin::widgets::catalog` keeps Mata-native counterparts for the requested GTK4 widget-class surface. These names are compatibility/API coverage names only; they are not GTK bindings and do not link GTK.

The catalog includes windows/dialog states, application/action controls, boxes, buttons/toggles/checks/switches, calendars, color/file/font controls, column/grid/list/tree views, drawing areas, dropdowns, editable/text widgets, popovers, fixed/flow/grid/overlay containers, frames, header bars, image/picture/SVG widgets, labels/inscriptions, level/progress/range/scale/spin controls, media controls, notebooks, paned/revealer/scrolling widgets, shortcut widgets, stacks, status widgets, video shells, viewports, and window-control states.

`Button` and `TextField` are the primary interactive MataWin software controls and are rendered by `WidgetWindow`.

`Audio`/`AudioSource` are Mata extensions for streamed playback.

`WebView` is the only widget allowed to use platform web-engine/toolkit code.
