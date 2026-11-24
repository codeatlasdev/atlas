// vk [-L layout] [-V variant] [-M model] [-d ms] item...   item: keysym | ctrl+keysym | shift+keysym.  Types with a real xkb keymap.
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <time.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "vk-proto.h"
static struct wl_seat *seat; static struct zwp_virtual_keyboard_manager_v1 *mgr;
static void reg(void *d, struct wl_registry *r, uint32_t n, const char *i, uint32_t v) {
  if (!strcmp(i, "wl_seat")) seat = wl_registry_bind(r, n, &wl_seat_interface, 1);
  else if (!strcmp(i, "zwp_virtual_keyboard_manager_v1")) mgr = wl_registry_bind(r, n, &zwp_virtual_keyboard_manager_v1_interface, 1);
}
static void rem(void *d, struct wl_registry *r, uint32_t n) {}
static const struct wl_registry_listener rl = { reg, rem };
static uint32_t now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1000 + t.tv_nsec / 1000000; }
static void press(struct zwp_virtual_keyboard_v1 *kb, struct wl_display *d, xkb_keycode_t kc, int down) {
  zwp_virtual_keyboard_v1_key(kb, now(), kc - 8, down ? WL_KEYBOARD_KEY_STATE_PRESSED : WL_KEYBOARD_KEY_STATE_RELEASED); wl_display_flush(d);
}
static int find(struct xkb_keymap *km, xkb_keysym_t ks, xkb_keycode_t *okc, xkb_level_index_t *olv, xkb_mod_mask_t *omask) {
  for (xkb_keycode_t kc = xkb_keymap_min_keycode(km); kc <= xkb_keymap_max_keycode(km); kc++)
    for (xkb_layout_index_t lay = 0; lay < xkb_keymap_num_layouts_for_key(km, kc); lay++)
      for (xkb_level_index_t lv = 0; lv < xkb_keymap_num_levels_for_key(km, kc, lay); lv++) {
        const xkb_keysym_t *syms; int n = xkb_keymap_key_get_syms_by_level(km, kc, lay, lv, &syms);
        if (n == 1 && syms[0] == ks) {
          xkb_mod_mask_t masks[4]; size_t nm = xkb_keymap_key_get_mods_for_level(km, kc, lay, lv, masks, 4);
          *okc = kc; *olv = lv; *omask = nm ? masks[0] : 0; return 1;
        }
      }
  return 0;
}
int main(int argc, char **argv) {
  const char *layout = "us", *variant = "", *model = "pc105"; int delay = 80, i = 1;
  for (; i < argc && argv[i][0] == '-' && argv[i][1] && !argv[i][2]; i += 2) {
    if (argv[i][1] == 'L') layout = argv[i+1]; else if (argv[i][1] == 'V') variant = argv[i+1];
    else if (argv[i][1] == 'M') model = argv[i+1]; else if (argv[i][1] == 'd') delay = atoi(argv[i+1]);
  }
  struct wl_display *dpy = wl_display_connect(NULL); if (!dpy) { fprintf(stderr, "no display\n"); return 1; }
  struct wl_registry *r = wl_display_get_registry(dpy); wl_registry_add_listener(r, &rl, NULL); wl_display_roundtrip(dpy);
  if (!seat || !mgr) { fprintf(stderr, "no seat/virtual keyboard\n"); return 1; }
  struct xkb_context *ctx = xkb_context_new(0);
  struct xkb_rule_names rn = { "evdev", model, layout, variant, "" };
  struct xkb_keymap *km = xkb_keymap_new_from_names(ctx, &rn, 0); if (!km) { fprintf(stderr, "bad keymap\n"); return 1; }
  char *s = xkb_keymap_get_as_string(km, XKB_KEYMAP_FORMAT_TEXT_V1); size_t len = strlen(s) + 1;
  int fd = memfd_create("km", 0); ftruncate(fd, len); void *m = mmap(NULL, len, PROT_WRITE, MAP_SHARED, fd, 0); memcpy(m, s, len);
  struct zwp_virtual_keyboard_v1 *kb = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(mgr, seat);
  zwp_virtual_keyboard_v1_keymap(kb, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, len); wl_display_roundtrip(dpy);
  usleep(300000);
  xkb_mod_mask_t shift = 1u << xkb_keymap_mod_get_index(km, "Shift"), ctrl = 1u << xkb_keymap_mod_get_index(km, "Control"), level3 = 1u << xkb_keymap_mod_get_index(km, "Mod5");
  (void)ctrl;
  for (; i < argc; i++) {
    char *it = argv[i]; int wantctrl = 0;
    if (!strncmp(it, "ctrl+", 5)) { wantctrl = 1; it += 5; }
    xkb_keysym_t ks = xkb_keysym_from_name(it, 0); if (ks == XKB_KEY_NoSymbol && strlen(it) == 1) ks = xkb_utf32_to_keysym((unsigned char)it[0]);
    if (ks == XKB_KEY_NoSymbol) { fprintf(stderr, "unknown keysym %s\n", it); return 1; }
    xkb_keycode_t kc; xkb_level_index_t lv; xkb_mod_mask_t mask;
    if (!find(km, ks, &kc, &lv, &mask)) { fprintf(stderr, "keysym %s not on layout %s\n", it, layout); return 1; }
    xkb_keycode_t mk[3]; int nmk = 0;
    if (wantctrl) { xkb_keycode_t c; xkb_level_index_t l; xkb_mod_mask_t m; if (find(km, XKB_KEY_Control_L, &c, &l, &m)) mk[nmk++] = c; }
    if (mask & shift) { xkb_keycode_t c; xkb_level_index_t l; xkb_mod_mask_t m; if (find(km, XKB_KEY_Shift_L, &c, &l, &m)) mk[nmk++] = c; }
    if (mask & level3) { xkb_keycode_t c; xkb_level_index_t l; xkb_mod_mask_t m; if (find(km, XKB_KEY_ISO_Level3_Shift, &c, &l, &m)) mk[nmk++] = c; }
    for (int j = 0; j < nmk; j++) { press(kb, dpy, mk[j], 1); usleep(30000); }
    press(kb, dpy, kc, 1); usleep(40000); press(kb, dpy, kc, 0); usleep(30000);
    for (int j = nmk - 1; j >= 0; j--) { press(kb, dpy, mk[j], 0); usleep(30000); }
    usleep(delay * 1000);
  }
  wl_display_roundtrip(dpy); usleep(200000); wl_display_roundtrip(dpy);
  return 0;
}
