// C wrapper for LVGL 9.x static-inline functions used by Rust FFI.
// LVGL 9.x style setters are static inline in headers; we export
// callable symbols here.
#include "lvgl.h"

// int32-valued style setters
void r1_lv_obj_set_style_pad_all(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_pad_all(o, v, s); }
void r1_lv_obj_set_style_pad_row(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_pad_row(o, v, s); }
void r1_lv_obj_set_style_pad_column(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_pad_column(o, v, s); }
void r1_lv_obj_set_style_radius(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_radius(o, v, s); }
void r1_lv_obj_set_style_border_width(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_border_width(o, v, s); }
void r1_lv_obj_set_style_bg_opa(lv_obj_t *o, int32_t v, lv_style_selector_t s) { lv_obj_set_style_bg_opa(o, v, s); }
// lv_color_t-valued
void r1_lv_obj_set_style_bg_color(lv_obj_t *o, lv_color_t v, lv_style_selector_t s) { lv_obj_set_style_bg_color(o, v, s); }
void r1_lv_obj_set_style_arc_color(lv_obj_t *o, lv_color_t v, lv_style_selector_t s) { lv_obj_set_style_arc_color(o, v, s); }
void r1_lv_obj_set_style_text_color(lv_obj_t *o, lv_color_t v, lv_style_selector_t s) { lv_obj_set_style_text_color(o, v, s); }
// Pointer-valued
void r1_lv_obj_set_style_text_font(lv_obj_t *o, const lv_font_t *v, lv_style_selector_t s) { lv_obj_set_style_text_font(o, v, s); }

lv_color_t r1_lv_color_hex(uint32_t hex) { return lv_color_hex(hex); }

void r1_lv_obj_set_flex_flow(lv_obj_t *o, lv_flex_flow_t f) { lv_obj_set_flex_flow(o, f); }
void r1_lv_obj_set_flex_align(lv_obj_t *o, lv_flex_align_t m, lv_flex_align_t c, lv_flex_align_t t) {
    lv_obj_set_flex_align(o, m, c, t);
}

// ── Constants (some LVGL enums aren't ABI-stable across versions) ──

#define EXPORT_CONST_INT(name, val) int r1_##name(void) { return val; }
#define EXPORT_CONST_U32(name, val) uint32_t r1_##name(void) { return val; }

EXPORT_CONST_INT(align_top_left,     LV_ALIGN_TOP_LEFT)
EXPORT_CONST_INT(align_top_right,    LV_ALIGN_TOP_RIGHT)
EXPORT_CONST_INT(align_top_mid,      LV_ALIGN_TOP_MID)
EXPORT_CONST_INT(align_bottom_left,  LV_ALIGN_BOTTOM_LEFT)
EXPORT_CONST_INT(align_bottom_mid,   LV_ALIGN_BOTTOM_MID)
EXPORT_CONST_INT(align_bottom_right, LV_ALIGN_BOTTOM_RIGHT)
EXPORT_CONST_INT(align_center,       LV_ALIGN_CENTER)
EXPORT_CONST_INT(align_out_bottom_left, LV_ALIGN_OUT_BOTTOM_LEFT)
EXPORT_CONST_INT(align_out_bottom_mid,  LV_ALIGN_OUT_BOTTOM_MID)

EXPORT_CONST_U32(flag_hidden,    LV_OBJ_FLAG_HIDDEN)
EXPORT_CONST_U32(flag_clickable, LV_OBJ_FLAG_CLICKABLE)
EXPORT_CONST_U32(flag_scrollable,LV_OBJ_FLAG_SCROLLABLE)

EXPORT_CONST_INT(event_clicked, LV_EVENT_CLICKED)
EXPORT_CONST_INT(event_gesture, LV_EVENT_GESTURE)

EXPORT_CONST_U32(part_main,      LV_PART_MAIN)
EXPORT_CONST_U32(part_indicator, LV_PART_INDICATOR)

EXPORT_CONST_U32(dir_left,  LV_DIR_LEFT)
EXPORT_CONST_U32(dir_right, LV_DIR_RIGHT)

EXPORT_CONST_U32(flex_flow_row,         LV_FLEX_FLOW_ROW)
EXPORT_CONST_U32(flex_flow_column,      LV_FLEX_FLOW_COLUMN)
EXPORT_CONST_U32(flex_align_start,      LV_FLEX_ALIGN_START)
EXPORT_CONST_U32(flex_align_center,     LV_FLEX_ALIGN_CENTER)
EXPORT_CONST_U32(flex_align_space_between, LV_FLEX_ALIGN_SPACE_BETWEEN)

int r1_anim_off(void) { return LV_ANIM_OFF; }

// ── Display helpers ────────────────────────────────────────

lv_disp_t *r1_disp_get_default(void) { return lv_disp_get_default(); }
int r1_disp_get_hor_res(void) {
    return lv_display_get_horizontal_resolution(lv_disp_get_default());
}
int r1_disp_get_ver_res(void) {
    return lv_display_get_vertical_resolution(lv_disp_get_default());
}

// ── Indev / Gesture ────────────────────────────────────────

uint32_t r1_indev_get_gesture_dir(lv_indev_t *indev) {
    return lv_indev_get_gesture_dir(indev);
}
lv_indev_t *r1_event_get_indev(lv_event_t *e) {
    return lv_event_get_indev(e);
}
