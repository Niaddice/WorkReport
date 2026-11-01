#!/usr/bin/env bash
# ============================================================================
# WorkReport 开发模式快速启停与构建脚本
#
# 用法：
#   ./dev.sh            启动开发模式（后台运行，等价于 start）
#   ./dev.sh start      启动开发模式
#   ./dev.sh stop       停止开发模式（结束 tauri dev 及其全部子进程）
#   ./dev.sh restart    重启开发模式
#   ./dev.sh build      重新构建前端、Rust 代码及桌面安装包
#   ./dev.sh status     查看运行状态
#   ./dev.sh logs       跟踪实时日志（Ctrl+C 退出跟踪）
# ============================================================================
set -euo pipefail

# 后台/非交互进程不会加载 shell 配置，显式补齐 rustup 与 Homebrew 的常见路径，
# 否则 tauri dev 会报 "failed to run 'cargo metadata' ... No such file or directory"。
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PID_FILE="$SCRIPT_DIR/.dev.pid"
LOG_FILE="$SCRIPT_DIR/.dev.log"
DEV_PORT=1420

info() { printf '\033[1;34m[dev]\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m[dev]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[dev]\033[0m %s\n' "$*"; }

# 递归结束一个进程及其全部子进程（先子后父，避免子进程被 init 收养后漏杀）
kill_tree() {
  local pid="$1" child
  for child in $(pgrep -P "$pid" 2>/dev/null); do
    kill_tree "$child"
  done
  kill "$pid" 2>/dev/null || true
}

# 读取 pid 文件中的主进程 PID；不存在或已失效则输出空
main_pid() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      echo "$pid"
    fi
  fi
}

is_running() { [[ -n "$(main_pid)" ]]; }

start() {
  if is_running; then
    warn "app 已在运行（PID $(main_pid)）"
    return 0
  fi

  # 清理失效的 pid 文件
  rm -f "$PID_FILE"

  info "启动 WorkReport 开发模式…"
  nohup npm run app:dev >"$LOG_FILE" 2>&1 &
  echo $! >"$PID_FILE"
  ok "已启动（PID $(cat "$PID_FILE")）"
  info "前端 dev 地址：http://localhost:$DEV_PORT"
  info "日志文件：${LOG_FILE}（用 ./dev.sh logs 跟踪）"
}

stop() {
  local pid
  pid="$(main_pid || true)"

  if [[ -n "$pid" ]]; then
    info "停止 app（PID ${pid}）…"
    kill_tree "$pid"
    # 等待优雅退出，最多约 2 秒
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.2
    done
    if kill -0 "$pid" 2>/dev/null; then
      warn "进程未及时退出，强制结束…"
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi

  # 兜底清理：只匹配本项目的明确进程，避免误杀其他项目的 vite 等
  pkill -f "tauri dev" 2>/dev/null || true
  pkill -f "app:dev" 2>/dev/null || true
  pkill -f "target/debug/work-report" 2>/dev/null || true

  rm -f "$PID_FILE"
  ok "已停止"
}

build() {
  info "重新构建 WorkReport…"
  npm run app:build
  ok "构建完成"
}

status() {
  if is_running; then
    ok "运行中（PID $(main_pid)）"
    if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$DEV_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
      info "前端 dev 服务监听：http://localhost:$DEV_PORT"
    fi
  else
    warn "未运行"
  fi
}

logs() {
  [[ -f "$LOG_FILE" ]] || { warn "日志文件不存在，先执行 ./dev.sh start"; return 1; }
  tail -n 100 -f "$LOG_FILE"
}

case "${1:-start}" in
  start)   start ;;
  stop)    stop ;;
  restart) stop && start ;;
  build)   build ;;
  status)  status ;;
  logs)    logs ;;
  *)
    echo "用法：$0 [start|stop|restart|build|status|logs]"
    exit 1
    ;;
esac
