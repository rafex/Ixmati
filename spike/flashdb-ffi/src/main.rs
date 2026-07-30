mod fdb {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

fn main() {
    println!("=== FlashDB FFI Spike ===");

    // Objetivo: inicializar FlashDB, hacer get/set/delete con TTL,
    // invalidación por prefijo, y documentar la superficie unsafe.
    // Ver README.md para criterios de evaluación completos.
    println!("build.rs exitoso: bindings generados");
    println!("pendiente: inicializar + benchmark vs sled");
}
