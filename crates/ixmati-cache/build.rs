#[cfg(feature = "flashdb")]
fn main() {
    use std::path::PathBuf;

    let flashdb = "flashdb-native";
    let inc = format!("{}/inc", flashdb);

    println!("cargo:rerun-if-changed={}/src/fdb.c", flashdb);
    println!("cargo:rerun-if-changed={}/src/fdb_kvdb.c", flashdb);
    println!("cargo:rerun-if-changed={}/src/fdb_tsdb.c", flashdb);
    println!("cargo:rerun-if-changed={}/src/fdb_file.c", flashdb);
    println!("cargo:rerun-if-changed={}/src/fdb_utils.c", flashdb);
    println!("cargo:rerun-if-changed={}/wrapper.h", flashdb);

    cc::Build::new()
        .include(&inc)
        .include(flashdb)
        .define("FDB_USING_KVDB", "1")
        .define("FDB_USING_FILE_LIBC_MODE", "1")
        .define("FDB_USING_TSDB", Some("1"))
        .define("FDB_WRITE_GRAN", Some("1"))
        .define("FDB_STRICT_ALIGN", Some("1"))
        .file(format!("{}/src/fdb.c", flashdb))
        .file(format!("{}/src/fdb_kvdb.c", flashdb))
        .file(format!("{}/src/fdb_tsdb.c", flashdb))
        .file(format!("{}/src/fdb_file.c", flashdb))
        .file(format!("{}/src/fdb_utils.c", flashdb))
        .compile("flashdb");

    let bindings = bindgen::Builder::default()
        .header(format!("{}/wrapper.h", flashdb))
        .clang_arg(format!("-I{}", inc))
        .clang_arg(format!("-I{}", flashdb))
        .allowlist_function("fdb_.*")
        .allowlist_type("fdb_.*")
        .allowlist_var("FDB_.*")
        .generate()
        .expect("unable to generate FlashDB bindings");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("flashdb_bindings.rs"))
        .expect("couldn't write bindings");
}

#[cfg(not(feature = "flashdb"))]
fn main() {}
