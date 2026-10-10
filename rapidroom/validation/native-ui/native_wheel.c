/* Native GTK scroll-event injector for the isolated test app only. */
#include <gtk/gtk.h>
#include <glib/gstdio.h>
#include <webkit2/webkit2.h>
#include <stdio.h>

static const char *command_path;

static GtkWidget *find_webview(GtkWidget *widget) {
    if (WEBKIT_IS_WEB_VIEW(widget)) return widget;
    if (!GTK_IS_CONTAINER(widget)) return NULL;
    GList *children = gtk_container_get_children(GTK_CONTAINER(widget));
    GtkWidget *result = NULL;
    for (GList *item = children; item && !result; item = item->next)
        result = find_webview(GTK_WIDGET(item->data));
    g_list_free(children);
    return result;
}

static gboolean poll_command(gpointer unused) {
    (void)unused;
    char *contents = NULL;
    if (!g_file_get_contents(command_path, &contents, NULL, NULL)) return G_SOURCE_CONTINUE;
    char direction;
    double x, y;
    if (sscanf(contents, "%c %lf %lf", &direction, &x, &y) != 3 || (direction != 'u' && direction != 'd')) {
        g_free(contents);
        return G_SOURCE_CONTINUE;
    }
    g_free(contents);
    GtkWidget *view = NULL;
    GList *windows = gtk_window_list_toplevels();
    for (GList *item = windows; item && !view; item = item->next)
        view = find_webview(GTK_WIDGET(item->data));
    g_list_free(windows);
    if (!view || !gtk_widget_get_realized(view)) return G_SOURCE_CONTINUE;
    GdkWindow *window = gtk_widget_get_window(view);
    GdkEvent *event = gdk_event_new(GDK_SCROLL);
    event->scroll.window = g_object_ref(window);
    event->scroll.send_event = FALSE;
    event->scroll.time = GDK_CURRENT_TIME;
    event->scroll.x = x;
    event->scroll.y = y;
    event->scroll.direction = direction == 'u' ? GDK_SCROLL_UP : GDK_SCROLL_DOWN;
    GdkDevice *pointer = gdk_seat_get_pointer(gdk_display_get_default_seat(gtk_widget_get_display(view)));
    if (pointer) {
        gdk_event_set_device(event, pointer);
        gdk_event_set_source_device(event, pointer);
    }
    int root_x, root_y;
    gdk_window_get_origin(window, &root_x, &root_y);
    event->scroll.x_root = root_x + x;
    event->scroll.y_root = root_y + y;
    g_unlink(command_path);
    gtk_widget_event(view, event);
    gdk_event_free(event);
    return G_SOURCE_CONTINUE;
}

__attribute__((constructor)) static void start_injector(void) {
    command_path = g_getenv("RAPIDROOM_TEST_NATIVE_WHEEL_FILE");
    if (command_path && *command_path) g_timeout_add(20, poll_command, NULL);
}
