# r1-panel (Rust + LVGL)

HS-NAS-R1 面板的 Rust 重写版本。使用 LVGL 直接渲染到 DRM/KMS 帧缓冲，替代 cog + WPEWebKit 浏览器方案。

## 架构对比

```
旧方案:
  Go 后端 (14 MB) → HTTP API → cog (96 MB) → WPEWebProcess (750+ MB)
  总内存: ~866 MB

新方案:
  Rust 单二进制 → LVGL → DRM 帧缓冲
  总内存: ~10 MB
```

## 构建

### 前置条件
```bash
# 克隆 LVGL C 库
cd rust-lvgl-panel
git submodule add https://github.com/lvgl/lvgl.git lvgl
cd lvgl && git checkout v9.2.0 && cd ..

# 构建 (需要 Linux + DRM 开发头文件)
cargo build --release
```

### NAS 上构建
```bash
# 在 NAS 上安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装依赖
apt install -y libdrm-dev pkg-config

# 构建
cd rust-lvgl-panel
git submodule update --init
cargo build --release
```

## 运行

```bash
# 停止旧的 Go 版
systemctl stop r1-panel

# 运行新版本
./target/release/r1-panel
```

## 项目结构

```
rust-lvgl-panel/
├── Cargo.toml          # Rust 项目配置
├── build.rs            # 编译 C LVGL 库
├── lv_conf.h           # LVGL 配置 (最小功能集)
├── lvgl/               # LVGL C 源码 (git submodule)
└── src/
    ├── main.rs         # 入口 + 事件循环
    ├── display.rs      # DRM/KMS 显示初始化
    ├── input.rs        # 触摸屏 evdev 输入
    ├── metrics.rs      # 系统指标采集 (/proc, /sys)
    └── ui.rs           # LVGL 界面 (2 页面板)
```

## 特性

- [x] CPU 使用率环形图 + 温度/频率
- [x] 内存使用率环形图
- [x] 网络速率 + IP 显示
- [x] 磁盘健康状态 + 使用率进度条
- [x] Docker 容器列表
- [x] 虚拟机列表
- [x] 核心服务状态
- [x] 重启/关机确认对话框
- [x] 触摸滑动手势切换页面
- [x] 5 秒自动刷新
- [x] 增量 UI 更新 (LVGL 原生支持)
