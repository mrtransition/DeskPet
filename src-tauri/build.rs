fn main() {
    tauri_build::build();
    // GNU 工具链适配：tauri-winres 在无 MSVC rc.exe 时会产出 .res 格式的 resource.lib，
    // MSVC link.exe 可直接链接该格式，但 GNU ld 无法识别。此处检测 .res 魔数（开头 4 字节为 0），
    // 用 windres 将其原地转换为 COFF 目标文件，GNU ld 即可正常链接。
    if cfg!(target_os = "windows") {
        let out = std::env::var("OUT_DIR").unwrap_or_default();
        if !out.is_empty() {
            let lib = std::path::Path::new(&out).join("resource.lib");
            if let Ok(bytes) = std::fs::read(&lib) {
                let is_res = bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 0 && bytes[3] == 0;
                let is_coff = bytes.len() >= 2 && (bytes[0] == 0x64 && bytes[1] == 0x86 || bytes[0] == 0x4C && bytes[1] == 0x01);
                if is_res && !is_coff {
                    let tmp_res = std::path::Path::new(&out).join("resource_converted.res");
                    if std::fs::copy(&lib, &tmp_res).is_ok() {
                        match std::process::Command::new("windres")
                            .arg("-i").arg(&tmp_res)
                            .arg("-o").arg(&lib)
                            .arg("-O").arg("coff")
                            .output()
                        {
                            Ok(st) if st.status.success() => {
                                println!("cargo:rerun-if-changed=build.rs");
                            }
                            _ => {
                                println!("cargo:warning=windres conversion failed; resource.lib remains .res format and linking may fail");
                            }
                        }
                        let _ = std::fs::remove_file(&tmp_res);
                    }
                }
            }
        }
    }
}
