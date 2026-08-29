# Web UI Component Sources

更新时间：2026-08-29

本项目的 Web 客户端保持原生 HTML/CSS/JS，触摸采集和 ATP1 帧编码不依赖 React、Tailwind 或运行时组件库。下面记录本轮调研后实际采用的组件模式和来源边界。

## 已整合的模式

| 项目 | 公开模式 | 本项目落地 | 许可证/边界 |
| --- | --- | --- | --- |
| [BeUI](https://beui.dev/) | Dock 的 gliding active pill、Bottom Sheet snap/inertia、Theme Toggle View Transition、Command Palette 键盘导航 | 原生 `#action-dock`、移动端 `#settings-sheet`、主题 `document.startViewTransition`、`#command-palette` | BeUI 页面标注 Free/Open Source，组件通过 shadcn 分发；本项目没有复制其 React/Framer Motion 源码，采用等价 DOM/CSS/JS 行为并保留来源链接，发布前仍需核对具体组件文件的许可证声明 |
| [shadcn/ui](https://ui.shadcn.com/) | Copy-paste、语义 HTML、可访问表单和可拥有的组件源码 | 使用 native `button`、`select`、`input`、`dialog` 语义，焦点环、`aria-*`、键盘操作和 reduced-motion 降级 | shadcn/ui 项目为 MIT；本项目未引入其 React/Tailwind 运行时 |
| [Transitions.dev](https://transitions.dev/) | 状态变化反馈、短时 spring/blur/scale 过渡 | 连接状态、深按确认进度、主题切换和命令面板的状态动效 | 采用交互原则和公开演示，不复制其专有页面实现 |
| [Rare UI](https://rareui.com/) | 稀有动效、单组件可复制、shadcn CLI 分发 | 作为动效克制度参考，避免把触控面做成模板化卡片 | 页面标注 Free/Open Source；具体组件许可证需在选用前逐项确认，本轮没有复制其源码 |
| [Beautiful UI](https://beautifului.dev/) | AI-native 状态组件、Task Row、Tool Chip、Approval/Loading 反馈 | 将“连接中/已连接/断开”视为明确状态，而不是静态装饰；为后续诊断面板保留同样的状态组件方向 | 页面公开展示组件和设计服务；本轮仅采用信息层级，不复制其实现 |

## Liquid Glass 实现来源

- [nikdelvin/liquid-glass](https://github.com/nikdelvin/liquid-glass)：MIT。其公开演示验证了以 SVG `feDisplacementMap` 为主、轻量 blur 为辅，并用 chromatic aberration/specular 纹理增强边缘的实现路径；本项目按同一公开技术方向改写为原生 HTML/CSS/JS。
- [PallavAg/liquid-glass-web-react](https://github.com/PallavAg/liquid-glass-web-react)：MIT。当前 Web 页的几何位移图和 SVG `feDisplacementMap` 路径沿用其公开方法，并改写为零依赖原生实现。
- [Liquid Glass in the Browser: Refraction with CSS and SVG](https://kube.io/blog/liquid-glass-css-svg)：公开技术说明。位移图使用红/绿通道表达 X/Y 折射，凸面 squircle 边缘场作为本项目的实现依据。
- [QWEA0/Liquid-Glass-Android](https://github.com/QWEA0/Liquid-Glass-Android)：MIT，Android 依赖为 `com.github.QWEA0:liquidglass:v2.0.2`。APK 继续使用其 View/AGSL/SDF 透镜、动态背景采样、色散和传感器高光。

## 实现约束

1. 组件只服务于控制层，不能覆盖或拦截触控面 pointer stream。
2. 所有玻璃表面都必须有 `prefers-reduced-transparency` 和高对比度回退。
3. 所有高频操作只做即时按压反馈，不加入会造成输入延迟的长动画。
4. Safari/Firefox 不支持 SVG filter 作为 live backdrop 时，使用普通 `backdrop-filter` 或不透明 surface，不宣称跨浏览器的真实折射。
5. 在正式发布前，将逐个锁定复制源码的组件许可证、NOTICE 和上游版本；本轮只记录调研和等价改写，不把未核验的第三方代码打包进产物。

## 主题 key

`light-glass`、`dark-glass`、`ocean-glass`、`sunset-glass`、`aurora-glass`、
`graphite-glass`、`custom-glass` 使用液态玻璃材质；
`tokyo-night`、`nord`、`dracula`、`solarized-dark`、`catppuccin-mocha`、
`monokai` 使用稳定编辑器表面；`classic-light`、`classic-dark`、
`high-contrast` 是系统/辅助表面。Android、Web 触控页和 Web 诊断页共享这
16 个可选 key；历史实验材质仍保留实现代码，但不会出现在选择器中，旧值会回退到
`light-glass`。
