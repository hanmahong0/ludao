// 阻止 Windows release 下弹控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lurus_desktop_lib::run()
}
