# r1-panel Rust 原生面板

海康威视 NAS R1 竖向触摸屏（`376×960 @ 56 Hz`）的 Rust 原生实现。运行版直接访问 DRM/KMS 和 evdev，不启动 Cog/WPE，也不依赖 LVGL。

## 当前功能

- CPU、内存环形仪表及温度/频率
- 网卡上下行速率和 IP 地址
- eMMC、NVMe、HDD 健康与容量
- 核心服务、Docker 容器和 libvirt 虚拟机状态
- 重启/关机确认弹窗
- 纵向滚动、惯性滚动、横向循环翻页和点击
- 双 dumb buffer + DRM page flip
- 快速数据每 `500 ms` 更新，慢速清单每 `5 s` 更新
- 快速刷新只覆盖动态卡片和数值，不清空整屏

## 模块

```text
src/
├── main.rs            # 启动、事件循环和电源操作
├── display.rs         # DRM/KMS、双缓冲、page flip、链路维护
├── input.rs           # evdev 多点触摸和手势识别
├── interaction.rs     # 滚动、惯性和切页动画状态
├── metrics.rs         # /proc、/sys 和系统命令的数据模型与采集器
├── metrics_worker.rs  # 500 ms 快采样和 5 s 慢采样后台线程
├── render.rs          # 软件栅格器、字体缓存和 BMP 输出
└── view.rs            # 固定布局、动态区域刷新和弹窗
```

`assets/emoji/` 中的 RGBA 图标通过 `include_bytes!` 编译进程序。`assets/emoji.ttf` 只用于 `tools/extract_emoji_bitmaps.py` 重新生成这些图标，不是运行时依赖。

## 构建与测试

```bash
source "$HOME/.cargo/env"
cd rust-lvgl-panel
cargo fmt --all --check
cargo check --locked --target x86_64-unknown-linux-gnu
cargo test --locked --target x86_64-unknown-linux-musl
cargo build --locked --release --target x86_64-unknown-linux-musl
```

发布产物：

```text
target/x86_64-unknown-linux-musl/release/r1-panel
```

程序依次尝试系统中的 Droid、DejaVu 和 Noto 字体。也可以通过 `R1_PANEL_FONT=/path/to/font.ttf` 指定字体。

## 截图模式

截图模式不打开 DRM 和 evdev：

```bash
R1_PANEL_PAGE=overview R1_PANEL_SCROLL=0 ./r1-panel --screenshot overview.bmp
R1_PANEL_PAGE=services R1_PANEL_SCROLL=900 ./r1-panel --screenshot services-bottom.bmp
R1_PANEL_MODAL=reboot ./r1-panel --screenshot modal.bmp
```

## 运行与稳定性

- 快速和慢速更新通道均为容量 1 的有界 channel，旧样本会被覆盖，不会无限积压。
- `smartctl`、`docker`、`virsh`、`systemctl` 等慢命令只在后台采集线程中执行。
- DRM 两个 dumb buffer 在退出时解除映射并销毁；链路维护线程共享停止标志。
- 动态更新不重建页面，只有慢清单或布局变化才触发整页重绘。
- 无触摸、动画和待更新数据时，主循环只进行轻量轮询。

## 分支说明

- `master`：当前 Rust 原生主线。
- `cog`：切换主线前的旧 Go + Cog 实现备份。
