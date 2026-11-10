fn main() {
    // 前端是编译时嵌进 exe 的：ui/dist 变了必须让 cargo 重新编译，否则只改前端时
    // `cargo build` 会直接跳过，跑起来的还是旧界面。
    println!("cargo:rerun-if-changed=../ui/dist");
    tauri_build::build()
}
