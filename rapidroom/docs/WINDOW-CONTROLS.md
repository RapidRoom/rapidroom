# Window controls on Linux

Settings → General → Window controls offers **Auto**, **Always show** and
**Hide** for the app's custom Linux titlebar. Close remains available in all
three modes. Native OS titlebars remain controlled by the compositor.

Auto hides Minimize only when the app is using native Wayland and its active
GDK display matches the socket in the positively detected Hyprland instance
lock. A stale Hyprland environment inherited by a private Weston compositor,
X11/XWayland, and unknown Wayland compositors retain their controls. Maximize
remains available for resizable windows. No other compositor is classified by
this change.

The maximize/restore icon follows GDK's granted maximized/fullscreen state.
The app listens to actual window-state changes, including ones that do not
resize the window. A rejected request does not optimistically change the icon.
GTK3's public API does not expose the per-window `xdg_toplevel.wm_capabilities`
event; this is a limited compositor policy with a user override, rather than
full protocol capability detection. GTK4 exposes a toplevel capabilities API.

References: [GTK3 maximize behavior](https://docs.gtk.org/gtk3/method.Window.maximize.html),
[Wayland protocol source](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/xdg-shell/xdg-shell.xml),
[GTK4 toplevel capabilities](https://docs.gtk.org/gdk4/flags.ToplevelCapabilities.html).

The native Hyprland evidence and human check must confirm that Minimize is a
no-op and Maximize is honoured on the tested compositor/version. Other
compositors keep their previous behavior. Physical input and compositor
shortcuts still need human testing.
