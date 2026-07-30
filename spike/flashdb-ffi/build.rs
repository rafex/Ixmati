use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/fdb.c");
    println!("cargo:rerun-if-changed=src/fdb_kvdb.c");
    println!("cargo:rerun-if-changed=src/fdb_tsdb.c");
    println!("cargo:rerun-if-changed=src/fdb_file.c");
    println!("cargo:rerun-if-changed=src/fdb_utils.c");
    println!("cargo:rerun-if-changed=inc/flashdb.h");
    println!("cargo:rerun-if-changed=wrapper.h");

    let mut build = cc::Build::new();

    build
        .include("inc")
        .include(".")
        .define("FDB_USING_KVDB", "1")
        .define("FDB_USING_FILE_LIBC_MODE", "1")
        .define("FDB_USING_TSDB", Some("1"))
        .define("FDB_WRITE_GRAN", Some("1"))
        .define("FDB_STRICT_ALIGN", Some("1"))
        .file("src/fdb.c")
        .file("src/fdb_kvdb.c")
        .file("src/fdb_tsdb.c")
        .file("src/fdb_file.c")
        .file("src/fdb_utils.c")
        .compile("flashdb");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg("-Iinc")
        .clang_arg("-I.")
        .allowlist_function("fdb_.*")
        .allowlist_type("fdb_.*")
        .allowlist_var("FDB_.*")
        .generate()
        .expect("unable to generate FlashDB bindings");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("couldn't write bindings");
}
