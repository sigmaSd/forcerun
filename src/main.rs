use std::fs::DirEntry;
use std::path::{Path, PathBuf};

const FORCE_RUN_PATH: &str = "/tmp/forcerun";
const ROOT_LIB: &str = "/usr/lib";

fn main() {
    let exe = std::env::args().nth(1).expect("No binary specified");
    let path = prepare_path(&exe);
    let libs_deps = ldd(&exe);
    let missing = missing_libs(&libs_deps);
    let system_libs = get_system_libs();

    missing
        .map(|lib| lib_root_name(lib, &system_libs))
        .for_each(|(lib_name, root_lib)| {
            if let Some(root_lib) = root_lib {
                let (_root_name, root_path) = root_lib;
                sym(&root_path, &path.join(lib_name));
            }
        });

    run(&exe, &path);
}

fn prepare_path(exe: &str) -> PathBuf {
    let exe_name = Path::new(exe)
        .file_name()
        .expect("Error reading executable name");
    let path = Path::new(FORCE_RUN_PATH).join(exe_name);
    if let Err(e) = std::fs::create_dir_all(&path) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            panic!(
                "Could not create directory: {} error: {}",
                path.display(),
                e
            );
        }
    }
    path
}

fn ldd(exe: &str) -> String {
    let out = String::from_utf8(
        std::process::Command::new("ldd")
            .arg(exe)
            .output()
            .expect("Error running ldd")
            .stdout,
    )
    .expect("somehow ldd output contains invalid utf8");
    out
}

fn missing_libs(missing_libs: &str) -> impl Iterator<Item = &str> {
    missing_libs
        .lines()
        .filter(|l| l.contains("not found"))
        .map(|l| {
            l.split("=>")
                .next()
                .expect("library with an empty name??")
                .trim()
        })
}

fn get_system_libs() -> Vec<DirEntry> {
    std::fs::read_dir(ROOT_LIB)
        .unwrap_or_else(|e| panic!("could not read file under {} error: {}", ROOT_LIB, e))
        .filter_map(Result::ok)
        .collect()
}

fn lib_root_name<'a, 'b>(
    lib_name: &'a str,
    system_libs: &'b [DirEntry],
) -> (&'a str, Option<(String, PathBuf)>) {
    //libtinfo.so.5 -> libtinfo.so.7 or libtinfo.so
    //libffi.so.6 -> libffi.so.8 or libffi.so
    let root_lib = || {
        let prefix_name = lib_name
            .rsplitn(2, '.')
            .nth(1)
            .unwrap_or_else(|| panic!("Could not find an appropriate root name for {}", lib_name));

        let condidates = system_libs.iter().filter_map(|e| {
            let file_name = e.file_name().into_string().ok()?;
            if file_name.starts_with(prefix_name) {
                Some((file_name, e.path()))
            } else {
                None
            }
        });

        // prefer libffi.so.7 over libffi.so for example
        let lib: Option<(String, PathBuf)> = condidates.fold(None, |acc, (name, path)| match acc {
            Some((acc_name, acc_path)) => {
                if name.len() > acc_name.len() {
                    Some((name, path))
                } else {
                    Some((acc_name, acc_path))
                }
            }
            None => Some((name, path)),
        });
        lib
    };
    (lib_name, root_lib())
}

fn sym(from: &Path, to: &Path) {
    if let Err(e) = std::os::unix::fs::symlink(from, to) {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            // ignore, user already run forcerun
            return;
        }
        eprintln!(
            "failed to symlink {} to {} error: {}",
            from.display(),
            to.display(),
            e,
        );
    }
}

fn run(exe: &str, path: &Path) {
    std::process::Command::new(exe)
        .args(&std::env::args().skip(2).collect::<Vec<String>>())
        .env("LD_LIBRARY_PATH", path)
        .spawn()
        .expect("failed to run the executable");
}
