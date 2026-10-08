# Android support

MataWin supports Android through `android-activity` 0.6.1 using its `NativeActivity` backend and the Android NDK native window API.

## Application crate

The final Android application should be built as a `cdylib` and should depend on MataWin normally:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
matawin = { path = "../matawin" }
```

MataWin enables `android-activity`'s `native-activity` backend on Android. The Android-only dependency set is isolated with `cfg(target_os = "android")`, so it is not built for Windows or Linux.

## Entry point

Android owns the top-level window. `Window::new` therefore cannot construct an Android Activity. Create the window from the `AndroidApp` passed to `android_main`:

```rust
use matawin::{AndroidApp, Framebuffer, Window, events::WindowEvent};

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let mut window = Window::from_android_app(app, "MataWin", (1280, 720));
    let mut framebuffer = Framebuffer::from_color(1280, 720, 0xff10_1820);
    let mut events = Vec::with_capacity(32);

    loop {
        window.poll_events_into(&mut events);

        if events.iter().any(|event| matches!(event, WindowEvent::Close)) {
            return;
        }

        for event in events.iter().copied() {
            if let WindowEvent::Resized(width, height) = event {
                if width != 0 && height != 0 {
                    framebuffer.resize_and_clear(width, height, 0xff10_1820);
                }
            }
        }

        window.present(&framebuffer);
    }
}
```

`poll_events_into` must be called regularly from the `android_main` thread. It pumps Android lifecycle and input messages as well as MataWin's portable event queue.

## Android manifest

A NativeActivity-based application needs a manifest entry for `android.app.NativeActivity`. The `android.app.lib_name` value is the final application's native library name, not `matawin` unless MataWin itself is the packaged application library.

```xml
<application android:hasCode="false" android:label="MataWin App">
    <activity
        android:name="android.app.NativeActivity"
        android:configChanges="orientation|keyboardHidden|screenSize"
        android:exported="true">
        <meta-data
            android:name="android.app.lib_name"
            android:value="my_app" />
        <intent-filter>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent-filter>
    </activity>
</application>
```

Use your normal Android packaging/build tooling to supply the SDK/NDK, ABI targets, resources, and manifest around the Rust `cdylib`.

## Rendering

`Window::present` locks the Activity's current `ANativeWindow` and writes an RGBA8888 software frame. MataWin pixels are converted from `0xAARRGGBB` to Android's R/G/B/A byte layout while copying. If the logical framebuffer size differs from the physical Activity surface, presentation uses nearest-neighbour scaling without changing Android's base surface dimensions.

This keeps `Window::client_size()` meaningful after device rotation or surface changes. `present_exact` still refuses to scale, which preserves the widget renderer's rerasterize-on-resize behavior.

## Input and lifecycle

Android lifecycle messages are translated into existing MataWin window events where possible. Physical key events are mapped to `Key`, pointer coordinates become `CursorMoved`, scroll axes become `Scroll`, and the primary contact is mirrored to `MouseLeft` so existing software widgets can be used with touch.

Native touch data is also available through:

```rust
let touches = window.poll_touches();
```

For lower-allocation loops, reuse a vector with `poll_touches_into`.

The touch queue is bounded so applications that use only the mouse-compatible stream do not accumulate touch records indefinitely.

## Fonts

`FontRenderer::system_default` and `FontRenderer::system_family` now search common Android font locations, including `/system/fonts`, `/product/fonts`, `/system_ext/fonts`, and `/vendor/fonts`. Common Roboto and Noto Sans files are tried first for the default font.

## Platform-specific behavior

Android controls Activity placement and top-level decorations, so `reposition`, `set_decorations`, `set_resizable`, `set_maximizable`, and `set_minimizable` are no-ops on Android. `set_title` and `set_icon` are also no-ops because those values normally come from Android application resources/manifest metadata.

The optional MataWin `webview` feature remains Windows/Linux-only and intentionally produces a clear compile error when enabled for Android. The native software widget set is supported.

`NativeActivity` does not provide full production-grade IME/text-input integration by itself. Physical key input works through the current MataWin key events; applications that need a complete Android soft-keyboard editing pipeline will need a future GameActivity/JNI text-input backend.
