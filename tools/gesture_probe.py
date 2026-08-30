#!/usr/bin/env python3
"""Interactive gesture probe — run this ON THE MAC while companion-net runs.

Each press of <Enter> fires exactly one synthetic touch gesture at the
running companion-net (127.0.0.1:4242), after printing what you should
see. Watch the screen, note pass/fail, keep pressing through the whole
list, and verify each gesture behaves as expected.

Usage:
    python3 tools/gesture_probe.py           # interactive (Enter per gesture)
    python3 tools/gesture_probe.py --auto    # 2s countdown between gestures

Prerequisites: companion-net running (`./target/release/companion-net -v`),
and for the scroll/pinch steps put a scrollable web page frontmost.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synthetic_sender as ss


def _out_and_back(sender):
    sender.animate(0.4, lambda t: [ss.contact(1, 22 + 40 * t, 30)], lift_extra=False)
    # Return leg at the same cadence — cursor ends where it started.
    sender.animate(0.4, lambda t: [ss.contact(1, 62 - 40 * t, 30)])


def make_steps(sender):
    send = lambda contacts, **kw: sender.send(contacts, **kw)
    anim = lambda dur, render: sender.animate(dur, render)
    R = args_stub()

    return [
        ("01 单指移动",
         "光标先向右滑、再滑回来（去程+回程，贴边也能看见）。",
         lambda: anim(0.4, lambda t: [ss.contact(1, 25 + 40 * t, 30)])
             if False else _out_and_back(sender)),
        ("02 滚动-慢速小幅",
         "页面内容轻微向下滚（手指下移=内容下移）。无惯性。",
         lambda: anim(0.5, lambda t: [ss.contact(1, 45, 15 + 18 * t),
                                      ss.contact(2, 45, 33 + 18 * t)]) or None,
         ),
        ("03 滚动-快速长距(带惯性)",
         "大幅滚动，松手后内容继续滑行一段。",
         lambda: ss.MODES["scroll"](sender, argwith(dur=0.35, dist=2.2))),
        ("04 双指右键(tap)",
         "弹出右键菜单（在光标当前位置）。⚠会点击",
         lambda: ss.MODES["right_tap"](sender, R)),
        ("05 右键菜单关闭提示",
         "如果上一步弹了菜单，按 Esc 关掉再回车继续。",
         lambda: None),
        ("06 三指上滑-慢速",
         "调度中心 Mission Control（窗口缩小平铺）。",
         lambda: ss.MODES["swipe3_up"](sender, argwith(dist=1.3, dur=0.45))),
        ("07 三指上滑-快速",
         "同上（真实触控板速度）。",
         lambda: ss.MODES["swipe3_up"](sender, argwith())),
        ("08 三指左滑",
         "切换到左边一个桌面空间。",
         lambda: ss.MODES["swipe3_left"](sender, argwith())),
        ("09 四指上滑",
         "同为 Mission Control（若三指已配成空间切换则此项无系统动作）。",
         lambda: ss.MODES["swipe4_up"](sender, argwith())),
        ("10 捏合放大",
         "前台上网页内容放大（需可缩放页面）。 Safari/Chrome 最明显。",
         lambda: ss.MODES["pinch_out"](sender, argwith())),
        ("11 捏合缩小",
         "缩小回去。",
         lambda: ss.MODES["pinch_in"](sender, argwith())),
        ("12 单击(tap)",
         "在光标当前位置左键单击。⚠会点击！请先把光标放到安全位置",
         lambda: ss.MODES["tap"](sender, R)),
        ("13 双击(tap×2)",
         "双击。⚠会点击",
         lambda: ss.MODES["doubletap"](sender, R)),
        ("14 收尾清屏状态",
         "无现象（复位空帧）。",
         lambda: ss.MODES["clear"](sender, R)),
    ]


def args_stub():
    class A:
        dist = 1.0
        dur = 0
        gap_ms = 120
        rate = 120.0
    return A()


def argwith(dist=1.0, dur=0):
    a = args_stub()
    a.dist, a.dur = dist, dur
    return a


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=4242)
    ap.add_argument("--rate", type=float, default=120.0)
    ap.add_argument("--auto", action="store_true", help="2s countdown instead of Enter")
    args = ap.parse_args()

    sock = __import__("socket").socket(__import__("socket").AF_INET, __import__("socket").SOCK_DGRAM)
    sender = ss.Sender(sock, (args.host, args.port), args.rate)

    print(f"target = udp://{args.host}:{args.port}\n"
          f"每步：看说明 → 回车触发 → 观察屏幕 → 记录通过/失败。\n"
          f"--auto 模式则自动倒计时。Ctrl-C 随时退出。\n")

    for i, item in enumerate(make_steps(sender)):
        name, expect, fire = item[0], item[1], item[-1]
        print(f"\n[{name}]")
        print(f"  预期: {expect}")
        try:
            if args.auto:
                print("  2 秒后发射…", flush=True)
                time.sleep(2)
            else:
                input("  回车发射 > ")
            sender.send([], lift_extra=True)   # always start lifted
            time.sleep(0.12)
            fire()
            print("  ✅ 已发送")
        except KeyboardInterrupt:
            print("\n中断"); sender.send([], lift_extra=True); return

    print("\n全部完成。请确认各项手势在 macOS 上的触发与响应是否符合预期。")


if __name__ == "__main__":
    main()
