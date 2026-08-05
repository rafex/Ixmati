# E2E Redb — Smoke Tests

> Fecha: 2026-08-05 | Versiones probadas: 2.6.3, 4.1.0

## Redb 2.6.3

**API**: `Database` (solo un tipo, sin ReadOnlyDatabase).
**Multi-proceso**: no soportado. File locking exclusivo.

```
Writer: Database::create() → OK
API:    Database::open()   → FAIL (No such file or directory / file locked)
```

Incluso con retry (15 intentos x 300ms), la API nunca puede abrir la DB
mientras el writer la tiene abierta.

## Redb 4.1.0

**API**: `Database` con `ReadableDatabase` trait (import requerido).
**ReadOnlyDatabase**: definido pero sin `begin_read()` en 4.1.0 (añadido en 4.5+).

```
Writer: Database::create() → OK
API:    Database::open()   → FAIL (file locked, mismo problema que 2.x)
```

## Veredicto

**FAIL** para multi-proceso con las versiones disponibles en crates.io (4.1.0 max).
Redb 4.5+ (no disponible) deberia tener `ReadOnlyDatabase` con `begin_read()`.

**Single-process**: funciona correctamente. `set/get/del/delete_by_prefix` todos operativos.

## Recomendacion

Redb es viable si:
- Se usa en single-process (writer y API en el mismo proceso)
- O se espera a que crates.io tenga redb >= 4.5 con ReadOnlyDatabase

Para el all-in-one actual (supervisord, multi-proceso), Redb no es viable.
