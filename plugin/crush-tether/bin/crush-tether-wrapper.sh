#!/bin/sh
# crush-tether 装载守卫 wrapper（design.md「插件分发形态与装载守卫」）：
# 保持哑——找到二进制则 exec 转发参数与 stdio/退出码（进程整体替换）；
# 缺席则 exit 2 + 安装指引（把失效模式 #2 的静默 fail-open 翻成响亮
# 失败）。永不解析 hook 信封。
# zcode 插件轨经 ${ZCODE_PLUGIN_ROOT} 绝对路径拉起本文件。Linux zcode
# 公测前的挂账件（hooks.json 当前指 .cmd，见 ROADMAP P8/M8.2）。
if command -v crush-tether >/dev/null 2>&1; then
  exec crush-tether "$@"
fi
echo "crush-tether: binary not found on PATH; install with: cargo install --path ." >&2
exit 2
