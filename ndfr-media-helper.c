/**
 * NDFR Media Helper
 * Needed for Scrubber Functionality
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <gio/gio.h>
#include <math.h>

// #define LOG(msg, ...) fprintf(stderr, "[ndfr-helper-log] " msg "\n", ##__VA_ARGS__)
#define LOG(msg, ...)

static GList* find_players(GDBusConnection *bus) {
    LOG("Finding players...");
    GList *players = NULL;
    GError *error = NULL;
    GVariant *reply;
    gchar **names;

    reply = g_dbus_connection_call_sync(bus, "org.freedesktop.DBus", "/org/freedesktop/DBus",
                                        "org.freedesktop.DBus", "ListNames", NULL, NULL,
                                        G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    if (error) {
        LOG("Error listing D-Bus names: %s", error->message);
        g_error_free(error);
        return NULL;
    }

    g_variant_get(reply, "(^as)", &names);
    g_variant_unref(reply);

    GList *playing_players = NULL;
    GList *paused_players = NULL;

    for (int i = 0; names[i]; i++) {
        if (g_str_has_prefix(names[i], "org.mpris.MediaPlayer2.")) {
            LOG("Found potential player: %s", names[i]);
            GDBusProxy *proxy = g_dbus_proxy_new_for_bus_sync(G_BUS_TYPE_SESSION, G_DBUS_PROXY_FLAGS_NONE, NULL,
                                                              names[i], "/org/mpris/MediaPlayer2",
                                                              "org.mpris.MediaPlayer2.Player", NULL, &error);
            if (error) {
                LOG("Error creating proxy for %s: %s", names[i], error->message);
                g_error_free(error);
                error = NULL;
                continue;
            }

            GVariant *status_variant = g_dbus_proxy_get_cached_property(proxy, "PlaybackStatus");
            if (status_variant) {
                const gchar *status = g_variant_get_string(status_variant, NULL);
                LOG("Player %s has status: %s", names[i], status);
                if (strcmp(status, "Playing") == 0) {
                    playing_players = g_list_append(playing_players, g_strdup(names[i]));
                } else if (strcmp(status, "Paused") == 0) {
                    paused_players = g_list_append(paused_players, g_strdup(names[i]));
                }
                g_variant_unref(status_variant);
            } else {
                LOG("Could not get PlaybackStatus for %s", names[i]);
            }
            g_object_unref(proxy);
        }
    }
    g_strfreev(names);

    players = g_list_concat(playing_players, paused_players);
    LOG("Finished finding players. Found %d active player(s).", g_list_length(players));
    return players;
}

static gchar* get_players_json(GDBusConnection *bus) {
    LOG("Getting players JSON...");
    GString *json_str = g_string_new("[");
    GList *players = find_players(bus);

    if (!players) {
        g_string_append(json_str, "]");
        LOG("No players found, returning empty JSON array.");
        return g_string_free(json_str, FALSE);
    }

    for (GList *l = players; l != NULL; l = l->next) {
        gchar *player_name = (gchar*)l->data;
        LOG("Processing player: %s", player_name);
        GError *error = NULL;
        GDBusProxy *player_proxy = g_dbus_proxy_new_for_bus_sync(G_BUS_TYPE_SESSION, G_DBUS_PROXY_FLAGS_NONE, NULL,
                                                                 player_name, "/org/mpris/MediaPlayer2",
                                                                 "org.mpris.MediaPlayer2.Player", NULL, &error);

        if (error || !player_proxy) {
            LOG("Failed to create proxy for player %s.", player_name);
            if (error) g_error_free(error);
            continue;
        }

        const gchar *status = "";
        gint64 position = 0, length = 0;
        const gchar *title = "", *icon_name = "";
        gchar *artist = NULL;

        GVariant *prop = g_dbus_proxy_get_cached_property(player_proxy, "PlaybackStatus");
        if (prop) {
            status = g_variant_get_string(prop, NULL);
            g_variant_unref(prop);
        }

        prop = g_dbus_proxy_get_cached_property(player_proxy, "Position");
        if (prop) {
            if (g_variant_is_of_type(prop, G_VARIANT_TYPE_INT64)) {
                position = g_variant_get_int64(prop);
            } else if (g_variant_is_of_type(prop, G_VARIANT_TYPE_UINT64)) {
                position = (gint64)g_variant_get_uint64(prop);
            }
            g_variant_unref(prop);
        }

        prop = g_dbus_proxy_get_cached_property(player_proxy, "Metadata");
        if (prop) {
            GVariantIter iter;
            gchar *key;
            GVariant *value;
            g_variant_iter_init(&iter, prop);
            while (g_variant_iter_next(&iter, "{sv}", &key, &value)) {
                if (strcmp(key, "mpris:length") == 0) {
                    if (g_variant_is_of_type(value, G_VARIANT_TYPE_INT64)) {
                        length = g_variant_get_int64(value);
                    } else if (g_variant_is_of_type(value, G_VARIANT_TYPE_UINT64)) {
                        length = (gint64)g_variant_get_uint64(value);
                    }
                } else if (strcmp(key, "xesam:title") == 0) {
                    title = g_variant_get_string(value, NULL);
                } else if (strcmp(key, "xesam:artist") == 0) {
                    if (g_variant_is_of_type(value, G_VARIANT_TYPE("as"))) {
                        const gchar **artists = g_variant_get_strv(value, NULL);
                        if (artists && artists[0]) {
                            artist = g_strdup(artists[0]);
                        }
                        // 'artists' should not be freed. it is still owned by GVariant `value`. Will cause memory corruption.
                    }
                }
                g_free(key);
                g_variant_unref(value);
            }
            g_variant_unref(prop);
        }

        if (g_str_has_prefix(player_name, "org.mpris.MediaPlayer2.")) {
            icon_name = player_name + strlen("org.mpris.MediaPlayer2.");
        }

        g_string_append_printf(json_str, "{\"player_id\":\"%s\",\"status\":\"%s\",\"position\":%lld,\"length\":%lld,\"title\":\"%s\",\"artist\":\"%s\",\"icon\":\"%s\"}",
               player_name, status, (long long)position, (long long)length, title, artist ? artist : "", icon_name);

        g_free(artist);
        g_object_unref(player_proxy);

        if (l->next) {
            g_string_append(json_str, ",");
        }
    }
    g_string_append(json_str, "]");
    g_list_free_full(players, g_free);
    LOG("Finished getting JSON: %s", json_str->str);
    return g_string_free(json_str, FALSE);
}

static int handle_get(GDBusConnection *bus) {
    LOG("Handling 'get' command...");
    gchar *json_data = get_players_json(bus);
    printf("%s\n", json_data);
    g_free(json_data);
    LOG("'get' command finished.");
    return 0;
}

static int handle_play_pause(GDBusConnection *bus, const char *player_id) {
    LOG("Handling 'play-pause' for player: %s", player_id ? player_id : "default");
    if (!player_id) return 1;
    g_dbus_connection_call_sync(bus, player_id, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player",
                                "PlayPause", NULL, NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL);
    return 0;
}

static int handle_set_position(GDBusConnection *bus, const char *player_id, const char *pos_str) {
    LOG("Handling 'set-position' for player %s to %s usecs", player_id, pos_str);
    if (!player_id) return 1;
    GError *error = NULL;
    GDBusProxy *proxy = g_dbus_proxy_new_for_bus_sync(G_BUS_TYPE_SESSION, G_DBUS_PROXY_FLAGS_NONE, NULL,
                                                      player_id, "/org/mpris/MediaPlayer2",
                                                      "org.mpris.MediaPlayer2.Player", NULL, &error);
    if (error) { g_error_free(error); return 1; }
    GVariant *can_seek_variant = g_dbus_proxy_get_cached_property(proxy, "CanSeek");
    if (!can_seek_variant || !g_variant_get_boolean(can_seek_variant)) {
        if (can_seek_variant) g_variant_unref(can_seek_variant);
        g_object_unref(proxy);
        return 1;
    }
    g_variant_unref(can_seek_variant);
    GVariant *pos_variant = g_dbus_proxy_get_cached_property(proxy, "Position");
    if (!pos_variant) { g_object_unref(proxy); return 1; }
    gint64 current_pos = g_variant_get_int64(pos_variant);
    g_variant_unref(pos_variant);
    gint64 offset = atoll(pos_str) - current_pos;
    g_dbus_proxy_call_sync(proxy, "Seek", g_variant_new("(x)", offset), G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    g_object_unref(proxy);
    if (error) { g_error_free(error); return 1; }
    return 0;
}

static int handle_set_position_percent(GDBusConnection *bus, const char *player_id, const char *percent_str) {
    LOG("Handling 'set-position-percent' for player %s to %s%%", player_id, percent_str);
    if (!player_id) return 1;
    GError *error = NULL;
    GDBusProxy *proxy = g_dbus_proxy_new_for_bus_sync(G_BUS_TYPE_SESSION, G_DBUS_PROXY_FLAGS_NONE, NULL,
                                                      player_id, "/org/mpris/MediaPlayer2",
                                                      "org.mpris.MediaPlayer2.Player", NULL, &error);
    if (error) { g_error_free(error); return 1; }
    GVariant *can_seek_variant = g_dbus_proxy_get_cached_property(proxy, "CanSeek");
    if (!can_seek_variant || !g_variant_get_boolean(can_seek_variant)) {
        if (can_seek_variant) g_variant_unref(can_seek_variant);
        g_object_unref(proxy);
        return 1;
    }
    g_variant_unref(can_seek_variant);
    double percentage = atof(percent_str);
    if (percentage < 0.0 || percentage > 100.0) { g_object_unref(proxy); return 1; }
    GVariant *metadata = g_dbus_proxy_get_cached_property(proxy, "Metadata");
    if (!metadata) { g_object_unref(proxy); return 1; }
    GVariant *length_variant = g_variant_lookup_value(metadata, "mpris:length", G_VARIANT_TYPE_INT64);
    g_variant_unref(metadata);
    if (!length_variant) { g_object_unref(proxy); return 1; }
    gint64 track_length = g_variant_get_int64(length_variant);
    g_variant_unref(length_variant);
    if (track_length <= 0) { g_object_unref(proxy); return 1; }
    GVariant *pos_variant = g_dbus_proxy_get_cached_property(proxy, "Position");
    if (!pos_variant) { g_object_unref(proxy); return 1; }
    gint64 current_pos = g_variant_get_int64(pos_variant);
    g_variant_unref(pos_variant);
    gint64 desired_pos = (gint64)((percentage / 100.0) * (double)track_length);
    gint64 offset = desired_pos - current_pos;
    g_dbus_proxy_call_sync(proxy, "Seek", g_variant_new("(x)", offset), G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    g_object_unref(proxy);
    if (error) { g_error_free(error); return 1; }
    return 0;
}

// --- 'listen' command implementation ---

static gboolean high_frequency_poll(gpointer user_data) {
    static gchar *previous_json = NULL;
    GDBusConnection *bus = (GDBusConnection *)user_data;

    gchar *current_json = get_players_json(bus);

    if (previous_json == NULL || g_strcmp0(previous_json, current_json) != 0) {
        LOG("State changed. New JSON: %s", current_json);
        printf("%s\n", current_json);
        fflush(stdout);
        
        g_free(previous_json);
        previous_json = current_json; // previous_json now owns the memory of current_json
    } else {
        g_free(current_json);
    }

    return G_SOURCE_CONTINUE;
}

static int handle_listen(GDBusConnection *bus) {
    LOG("Handling 'listen' command using high-frequency polling");
    GMainLoop *loop = g_main_loop_new(NULL, FALSE);

    g_timeout_add(200, high_frequency_poll, bus);

    LOG("Starting GMainLoop for polling...");
    g_main_loop_run(loop);

    LOG("GMainLoop finished");
    g_main_loop_unref(loop);
    return 0;
}

int main(int argc, char *argv[]) {
    LOG("ndfr-media-helper started with %d args", argc);
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <command> [player_id] [args...]\n", argv[0]);
        fprintf(stderr, "Commands:\n  get\n  listen\n  play-pause [player_id]\n  set-position [player_id] <usecs>\n  set-position-percent [player_id] <%%>\n");
        return 1;
    }
    GDBusConnection *bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, NULL);
    if (!bus) {
        LOG("Failed to connect to D-Bus session bus.");
        return 1;
    }
    LOG("Successfully connected to D-Bus session bus.");

    const char *command = argv[1];
    int result = 1;
    if (strcmp(command, "get") == 0) {
        result = handle_get(bus);
    } else if (strcmp(command, "listen") == 0) {
        result = handle_listen(bus);
    } else if (strcmp(command, "play-pause") == 0) {
        const char *player = NULL;
        GList *players = NULL;
        if (argc > 2) {
            player = argv[2];
        } else {
            players = find_players(bus);
            if (players) {
                player = (const char*)players->data;
            }
        }
        if (player) {
            result = handle_play_pause(bus, player);
        } else {
            LOG("play-pause: No player found or specified.");
        }
        if (players) {
            g_list_free_full(players, g_free);
        }
    } else if (strcmp(command, "set-position") == 0) {
        if (argc > 3) {
            result = handle_set_position(bus, argv[2], argv[3]);
        } else if (argc > 2) {
            GList *players = find_players(bus);
            if (players) {
                result = handle_set_position(bus, (const char*)players->data, argv[2]);
                g_list_free_full(players, g_free);
            } else {
                LOG("set-position: No player found to apply position to.");
            }
        }
    } else if (strcmp(command, "set-position-percent") == 0) {
        if (argc > 3) {
            result = handle_set_position_percent(bus, argv[2], argv[3]);
        } else if (argc > 2) {
            GList *players = find_players(bus);
            if (players) {
                result = handle_set_position_percent(bus, (const char*)players->data, argv[2]);
                g_list_free_full(players, g_free);
            }
        } else {
            LOG("set-position-percent: No player found to apply position to.");
        }
    } else {
        LOG("Unknown command: %s", command);
    }
    g_object_unref(bus);
    LOG("ndfr-media-helper finished with result: %d.");
    return result;
}
