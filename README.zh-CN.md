<p align="center">
  <img src="assets/tervia.svg" alt="Tervia logo" width="88" />
</p>

<h1 align="center">Tervia</h1>

<p align="center">让终端里的 AI 编程，少一点来回切换。</p>

<p align="center">
  <a href="https://github.com/murongg/Tervia/releases"><img src="https://img.shields.io/github/v/release/murongg/Tervia?style=flat&amp;labelColor=252b36&amp;color=a8d9c5" alt="GitHub release" /></a>
  <a href="https://github.com/murongg/Tervia/stargazers"><img src="https://img.shields.io/github/stars/murongg/Tervia?style=flat&amp;labelColor=252b36&amp;color=a8d9c5" alt="GitHub stars" /></a>
  <a href="docs/guide.zh-CN.md"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-667388?style=flat&amp;labelColor=252b36" alt="Platforms: macOS, Windows, Linux" /></a>
</p>

[English](README.md) | 简体中文

## 为什么做 Tervia

用 Claude Code、Codex 这类终端工具写代码时，运行命令、查看改动、编辑文件往往分散在几个窗口里。项目一多，还得反复找回对应的终端和工作进度。

Tervia 把终端、Git 和轻量文件编辑放进同一个桌面工作区，让你围绕一个项目完成编码、检查、修改和提交。

## 基础功能

- **终端与 AI 工具**：无需选择项目即可使用独立本地终端。沿用熟悉的 Shell，支持多标签、分屏和输出搜索，自动识别 Claude Code、Codex 等工具并显示对应图标。
- **项目任务**：发现当前工作目录的 package.json 脚本、Cargo、Makefile、justfile 和 Taskfile 任务，搜索后在独立终端运行，保留输出和退出结果，并可重新运行。
- **Agent 状态与通知**：连接 Claude Code、Codex 生命周期钩子，显示处理、等待授权或输入、本轮结束等状态；后台桌面通知可直接返回对应终端。
- **Git 管理**：查看代码改动，按文件或部分改动暂存、提交，浏览历史、切换分支和处理冲突。
- **SSH 管理**：保存常用主机、读取 SSH 配置别名，左侧选择主机、右侧直接使用远程终端，沿用系统密钥与 ssh-agent。
- **文件管理与编辑**：新建、重命名、复制、移动文件和文件夹，删除时移到废纸篓；快速查找文件、搜索项目内容，支持语法高亮、Markdown 阅读和图片预览。
- **多项目切换**：收藏常用项目，各自保留终端、文件和草稿，也能通过 Git worktree 并行处理不同任务。
- **复制代码上下文**：选中代码，连同文件路径和行号一起复制，方便交给 AI 或与他人讨论。
- **界面偏好**：提供 Darcula、Monokai、Solarized、Quiet Light 等十一种主题，深浅配色独立保存，支持跟随系统，可复制主题并可视化调色与实时预览，调整字体和字号，支持中文、英文等十种界面语言。

## 开始使用

目前优先打磨 macOS 15+，Windows / Linux 体验仍在完善中。

[下载体验](https://github.com/murongg/Tervia/releases) · [使用指南](docs/guide.zh-CN.md) · [更新记录](CHANGELOG.md) · [反馈建议](https://github.com/murongg/Tervia/issues)

## 开源协议

Tervia 采用 [MIT 许可证](LICENSE)。

第三方依赖和素材保留各自的许可协议。
