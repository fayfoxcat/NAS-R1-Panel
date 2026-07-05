// Build script: compiles LVGL 9.x C library and links it.
//
// Prerequisites:
//   git submodule add https://github.com/lvgl/lvgl.git lvgl
//   cd lvgl && git checkout v9.2.0
//
// LVGL is compiled as a static library with:
//   - Software rendering (we provide a framebuffer via DRM)
//   - No GPU/OpenGL (renders CPU-side into DRM dumb buffer)
//   - Linux evdev input
//   - Minimal feature set for this panel

use std::env;
use std::path::PathBuf;

fn main() {
    let lvgl_dir = PathBuf::from("lvgl");
    let lvgl_src = lvgl_dir.join("src");

    if !lvgl_src.exists() {
        panic!(
            "LVGL source not found at {}. Run:\n  git submodule add https://github.com/lvgl/lvgl.git lvgl\n  cd lvgl && git checkout v9.2.0",
            lvgl_dir.display()
        );
    }

    // Generate lv_conf.h if it doesn't exist
    let conf_path = PathBuf::from("lv_conf.h");
    if !conf_path.exists() {
        std::fs::write(&conf_path, LV_CONF_H).expect("Failed to write lv_conf.h");
        println!("cargo:warning=Generated lv_conf.h with default settings");
    }

    // Collect all LVGL C source files
    let mut cc = cc::Build::new();
    cc.include(&lvgl_dir)
        .include(".") // for lv_conf.h
        .define("LV_CONF_INCLUDE_SIMPLE", "1")
        .file(lvgl_src.join("core/lv_obj.c"))
        .file(lvgl_src.join("core/lv_obj_class.c"))
        .file(lvgl_src.join("core/lv_obj_style.c"))
        .file(lvgl_src.join("core/lv_obj_pos.c"))
        .file(lvgl_src.join("core/lv_obj_scroll.c"))
        .file(lvgl_src.join("core/lv_obj_draw.c"))
        .file(lvgl_src.join("core/lv_obj_tree.c"))
        .file(lvgl_src.join("core/lv_obj_event.c"))
        .file(lvgl_src.join("core/lv_group.c"))
        .file(lvgl_src.join("core/lv_indev.c"))
        .file(lvgl_src.join("core/lv_indev_scroll.c"))
        .file(lvgl_src.join("core/lv_disp.c"))
        .file(lvgl_src.join("core/lv_refr.c"))
        .file(lvgl_src.join("core/lv_theme.c"))
        .file(lvgl_src.join("draw/lv_draw.c"))
        .file(lvgl_src.join("draw/lv_draw_arc.c"))
        .file(lvgl_src.join("draw/lv_draw_buf.c"))
        .file(lvgl_src.join("draw/lv_draw_label.c"))
        .file(lvgl_src.join("draw/lv_draw_line.c"))
        .file(lvgl_src.join("draw/lv_draw_mask.c"))
        .file(lvgl_src.join("draw/lv_draw_rect.c"))
        .file(lvgl_src.join("draw/lv_draw_triangle.c"))
        .file(lvgl_src.join("draw/lv_draw_vector.c"))
        .file(lvgl_src.join("draw/lv_image_decoder.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_arc.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_border.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_box_shadow.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_fill.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_gradient.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_img.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_letter.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_line.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_mask.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_triangle.c"))
        .file(lvgl_src.join("draw/sw/lv_draw_sw_vector.c"))
        .file(lvgl_src.join("font/lv_font.c"))
        .file(lvgl_src.join("font/lv_font_fmt_txt.c"))
        .file(lvgl_src.join("font/lv_font_montserrat_24.c"))
        .file(lvgl_src.join("misc/lv_anim.c"))
        .file(lvgl_src.join("misc/lv_area.c"))
        .file(lvgl_src.join("misc/lv_color.c"))
        .file(lvgl_src.join("misc/lv_color_op.c"))
        .file(lvgl_src.join("misc/lv_ll.c"))
        .file(lvgl_src.join("misc/lv_log.c"))
        .file(lvgl_src.join("misc/lv_math.c"))
        .file(lvgl_src.join("misc/lv_palette.c"))
        .file(lvgl_src.join("misc/lv_style.c"))
        .file(lvgl_src.join("misc/lv_style_gen.c"))
        .file(lvgl_src.join("misc/lv_templ.c"))
        .file(lvgl_src.join("misc/lv_timer.c"))
        .file(lvgl_src.join("misc/lv_text.c"))
        .file(lvgl_src.join("misc/lv_utils.c"))
        .file(lvgl_src.join("stdlib/lv_mem.c"))
        .file(lvgl_src.join("stdlib/lv_string.c"))
        .file(lvgl_src.join("widgets/arc/lv_arc.c"))
        .file(lvgl_src.join("widgets/bar/lv_bar.c"))
        .file(lvgl_src.join("widgets/button/lv_button.c"))
        .file(lvgl_src.join("widgets/buttonmatrix/lv_buttonmatrix.c"))
        .file(lvgl_src.join("widgets/label/lv_label.c"))
        .file(lvgl_src.join("others/gridnav/lv_gridnav.c"))
        .flag_if_supported("-std=c11")
        .flag_if_supported("-Wall")
        .flag_if_supported("-O2")
        .compile("lvgl");

    // Link system libraries
    println!("cargo:rustc-link-lib=m"); // math library

    println!("cargo:rerun-if-changed=lv_conf.h");
    println!("cargo:rerun-if-changed=lvgl/");
}

const LV_CONF_H: &str = r#"
/**
 * LVGL Configuration for r1-panel (Rust + DRM).
 * Minimal feature set: software rendering to memory buffer.
 */
#ifndef LV_CONF_H
#define LV_CONF_H

#ifdef __cplusplus
extern "C" {
#endif

/*── Color ───────────────────────────────*/
#define LV_COLOR_DEPTH 32
#define LV_COLOR_CHROMA_KEY lv_color_hex(0xFF00FF)
#define LV_COLOR_SCREEN_TRANSP 0

/*── Memory ──────────────────────────────*/
#define LV_MEM_SIZE (2 * 1024 * 1024)  /* 2 MB heap */
#define LV_MEM_CUSTOM 0

/*── Display buffer ──────────────────────*/
#define LV_DRAW_BUF_STRIDE_ALIGN 1
#define LV_DRAW_BUF_ALIGN 4

/*── OS / Tick ───────────────────────────*/
#define LV_TICK_CUSTOM 0
#define LV_USE_TIMER 0

/*── Features ────────────────────────────*/
#define LV_USE_LOG 1
#define LV_LOG_LEVEL LV_LOG_LEVEL_WARN
#define LV_USE_ASSERT_NULL 0
#define LV_USE_ASSERT_MALLOC 0
#define LV_USE_ASSERT_STYLE 0
#define LV_USE_ASSERT_MEM_INTEGRITY 0
#define LV_USE_ASSERT_OBJ 0

/*── Drawing ─────────────────────────────*/
#define LV_DRAW_SW_COMPLEX 1
#define LV_SHADOW_CACHE_SIZE 0
#define LV_IMAGE_TRANSFORM_ENABLE 0
#define LV_CIRCLE_CACHE_SIZE 4
#define LV_USE_VECTOR_GRAPHIC 0

/*── GPU ─────────────────────────────────*/
#define LV_USE_DRAW_SW 1
#define LV_USE_GPU 0

/*── Fonts ───────────────────────────────*/
#define LV_FONT_MONTSERRAT_24 1

/*── Widgets ─────────────────────────────*/
#define LV_USE_ARC 1
#define LV_USE_BAR 1
#define LV_USE_BUTTON 1
#define LV_USE_BUTTONMATRIX 0
#define LV_USE_LABEL 1
#define LV_USE_IMAGE 0
#define LV_USE_SLIDER 0
#define LV_USE_SWITCH 0
#define LV_USE_TEXTAREA 0
#define LV_USE_TABLE 0
#define LV_USE_ANIMIMG 0
#define LV_USE_CALENDAR 0
#define LV_USE_CHART 0
#define LV_USE_CHECKBOX 0
#define LV_USE_DROPDOWN 0
#define LV_USE_LINE 0
#define LV_USE_ROLLER 0
#define LV_USE_SCALE 0
#define LV_USE_TABVIEW 0
#define LV_USE_TILEVIEW 0
#define LV_USE_WIN 0

/*── Layouts ─────────────────────────────*/
#define LV_USE_FLEX 1
#define LV_USE_GRID 0

/*── Others ──────────────────────────────*/
#define LV_USE_SNAPSHOT 0
#define LV_USE_MONKEY 0
#define LV_USE_GRIDNAV 0
#define LV_USE_FRAGMENT 0
#define LV_USE_MSG 0
#define LV_USE_IME_PINYIN 0
#define LV_USE_FILE_EXPLORER 0

/*── Theme ───────────────────────────────*/
#define LV_USE_THEME_DEFAULT 1
#define LV_THEME_DEFAULT_DARK 1
#define LV_THEME_DEFAULT_GROW 1

/*── Built-in fonts ──────────────────────*/
#define LV_FONT_DEFAULT &lv_font_montserrat_24

#ifdef __cplusplus
}
#endif

#endif /* LV_CONF_H */
"#;
