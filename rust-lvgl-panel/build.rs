// Build script: compiles LVGL 9.x C library and links it.
//
// LVGL is compiled as a static library with:
//   - Software rendering (LV_USE_DRAW_SW=1)
//   - No GPU (renders CPU-side into DRM dumb buffer)
//   - Linux pthread + evdev input
//   - Minimal feature set for this panel

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let lvgl_dir = PathBuf::from("lvgl");
    let lvgl_src = lvgl_dir.join("src");

    if !lvgl_src.exists() {
        panic!(
            "LVGL source not found at {}. Run:\n  git submodule add https://github.com/lvgl/lvgl.git lvgl",
            lvgl_dir.display()
        );
    }

    // Generate lv_conf.h if it doesn't exist
    let conf_path = PathBuf::from("lv_conf.h");
    if !conf_path.exists() {
        fs::write(&conf_path, LV_CONF_H).expect("Failed to write lv_conf.h");
        println!("cargo:warning=Generated lv_conf.h");
    }

    // Collect all LVGL C source files (recursive)
    let mut sources: Vec<PathBuf> = Vec::new();
    collect_c_files(&lvgl_src, &mut sources);

    // Filter out platform-specific files we don't need
    let skip_prefixes = [
        "draw/dma2d/", "draw/espressif/", "draw/eve/", "draw/nanovg/",
        "draw/nema_gfx/", "draw/nxp/", "draw/renesas/", "draw/vg_lite/",
        "draw/sdl/", "draw/opengles/",
        "drivers/display/", "drivers/nuttx/", "drivers/opengles/",
        "drivers/qnx/", "drivers/sdl/", "drivers/uefi/",
        "drivers/wayland/", "drivers/windows/", "drivers/x11/",
        "osal/lv_cmsis_rtos2.c", "osal/lv_freertos.c", "osal/lv_mqx.c",
        "osal/lv_os_none.c", "osal/lv_rtthread.c", "osal/lv_sdl2.c",
        "osal/lv_windows.c",
        "stdlib/builtin/", "stdlib/clib/", "stdlib/micropython/",
        "stdlib/rtthread/", "stdlib/uefi/",
        "libs/barcode/", "libs/bin_decoder/", "libs/bmp/", "libs/ffmpeg/",
        "libs/freetype/", "libs/frogfs/", "libs/fsdrv/", "libs/FT800-FT813/",
        "libs/gif/", "libs/gltf/", "libs/gstreamer/", "libs/libjpeg_turbo/",
        "libs/libpng/", "libs/libwebp/", "libs/lodepng/", "libs/lz4/",
        "libs/nanovg/", "libs/qrcode/", "libs/rle/", "libs/rlottie/",
        "libs/svg/", "libs/tiny_ttf/", "libs/tjpgd/",
        "libs/vg_lite_driver/",
        "debugging/", // skip test/debug files
    ];

    let skip_files = [
        "misc/lv_profiler_builtin.c", "misc/lv_profiler_builtin_posix.c",
        "font/lv_font_dejavu_16_persian_hebrew.c",
        "font/lv_font_source_han_sans_sc_14_cjk.c",
        "font/lv_font_source_han_sans_sc_16_cjk.c",
    ];

    // Font files we need
    let keep_fonts = ["lv_font.c", "lv_font_montserrat_24.c", "lv_font_fmt_txt.c", "lv_imgfont.c", "lv_binfont_loader.c"];
    let fonts_needed = ["font/fmt_txt/lv_font_fmt_txt.c", "font/lv_font_montserrat_24.c"];

    let filtered = sources.iter().filter(|p| {
        let path_str = p.to_string_lossy();
        let rel = path_str.strip_prefix(&format!("{}/", lvgl_src.display())).unwrap_or(&path_str);

        for prefix in &skip_prefixes {
            if rel.starts_with(prefix) { return false; }
        }
        for file in &skip_files {
            if rel == *file { return false; }
        }

        // Skip all font files except the ones we need
        if rel.starts_with("font/") && !fonts_needed.contains(&rel) {
            if !keep_fonts.iter().any(|f| rel.ends_with(f)) {
                return false;
            }
        }

        // Skip all libs except those needed
        if rel.starts_with("libs/") { return false; }

        // Skip non-Linux osal
        if rel.starts_with("osal/") && rel != "osal/lv_os.c" && rel != "osal/lv_linux.c" && rel != "osal/lv_pthread.c" {
            return false;
        }

        // Skip non-pthread stdlib
        if rel == "stdlib/lv_mem.c" || rel.starts_with("stdlib/builtin/") {
            return true;
        }
        if rel.starts_with("stdlib/") && !rel.starts_with("stdlib/builtin/") && rel != "stdlib/lv_mem.c" {
            return false;
        }

        // Skip widget property files (they cause duplicate symbol errors)
        if rel.starts_with("widgets/property/") { return false; }

        // Keep only the widgets we use
        if rel.starts_with("widgets/") {
            let keep_widgets = [
                "arc/lv_arc.c", "bar/lv_bar.c", "button/lv_button.c",
                "buttonmatrix/lv_buttonmatrix.c", "label/lv_label.c",
            ];
            return keep_widgets.iter().any(|w| rel.ends_with(w));
        }

        // Skip other large subdirectories
        if rel.starts_with("others/") && !rel.contains("gridnav") && !rel.contains("fragment") {
            return false;
        }

        true
    });

    let mut cc = cc::Build::new();
    cc.include(&lvgl_dir)                       // for lvgl.h (root)
        .include(lvgl_dir.join("include"))      // for lvgl/core/ etc
        .include(lvgl_dir.join("src"))          // for internal includes
        .include(".")                            // for lv_conf.h
        .define("LV_CONF_INCLUDE_SIMPLE", "1")
        .flag_if_supported("-std=c11")
        .flag_if_supported("-O2");

    for src in filtered {
        cc.file(&src);
    }

    cc.compile("lvgl");

    // Link system libraries
    println!("cargo:rustc-link-lib=m");     // math
    println!("cargo:rustc-link-lib=pthread"); // pthread

    println!("cargo:rerun-if-changed=lv_conf.h");
    println!("cargo:rerun-if-changed=lvgl/");
}

fn collect_c_files(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_c_files(&path, out);
            } else if path.extension().map_or(false, |e| e == "c") {
                out.push(path);
            }
        }
    }
}

const LV_CONF_H: &str = r#"
#ifndef LV_CONF_H
#define LV_CONF_H

#ifdef __cplusplus
extern "C" {
#endif

/*── Color ───────────────────────────────*/
#define LV_COLOR_DEPTH 32

/*── Memory ──────────────────────────────*/
#define LV_MEM_SIZE (2 * 1024 * 1024)

/*── Display buffer ──────────────────────*/
#define LV_DRAW_BUF_STRIDE_ALIGN 1
#define LV_DRAW_BUF_ALIGN 4

/*── OS / Tick ───────────────────────────*/
#define LV_TICK_CUSTOM 0
#define LV_USE_TIMER 0

/*── Logging ─────────────────────────────*/
#define LV_USE_LOG 1
#define LV_LOG_LEVEL LV_LOG_LEVEL_WARN
#define LV_USE_ASSERT_NULL 0
#define LV_USE_ASSERT_MALLOC 0

/*── Drawing ─────────────────────────────*/
#define LV_DRAW_SW_COMPLEX 1
#define LV_SHADOW_CACHE_SIZE 0
#define LV_IMAGE_TRANSFORM_ENABLE 0
#define LV_CIRCLE_CACHE_SIZE 4

/*── GPU ─────────────────────────────────*/
#define LV_USE_DRAW_SW 1
#define LV_USE_GPU 0

/*── Fonts ───────────────────────────────*/
#define LV_FONT_MONTSERRAT_24 1

/*── Widgets ─────────────────────────────*/
#define LV_USE_ARC 1
#define LV_USE_BAR 1
#define LV_USE_BUTTON 1
#define LV_USE_BUTTONMATRIX 1
#define LV_USE_KEYBOARD 0
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
#define LV_USE_LED 0
#define LV_USE_SPINNER 0
#define LV_USE_LIST 0
#define LV_USE_MENU 0
#define LV_USE_MSGBOX 0
#define LV_USE_SPAN 0
#define LV_USE_SPINBOX 0
#define LV_USE_CANVAS 0

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

/*── Font ────────────────────────────────*/
#define LV_FONT_DEFAULT &lv_font_montserrat_24

#ifdef __cplusplus
}
#endif

#endif /* LV_CONF_H */
"#;
