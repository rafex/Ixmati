# Validación exacta del SHA `b023819`: 10/s — no cerrada

## Resultado

La ejecución se inició contra los artefactos amd64 construidos desde el SHA
exacto publicado en `origin/main` (`b023819a32a22863920f10b502bf929b3a79c160`).
Se interrumpió después de observar el fallo del criterio durable; no es una
prueba de una hora válida.

- Escenario: `production-profile-10s-1h`
- Tasa objetivo: `10/s`
- Concurrencia: `200`
- Generador: `ixmati-protocol-bench` dentro de un contenedor Debian separado
- API: REST/JSON, `ack_mode=committed`
- `BATCH_INTERVAL_MS=100`
- Timeout de confirmación durable: `2000 ms`
- Inicio: `2026-08-13T00:15:35Z`
- Primer `PENDING` observado: `2026-08-13T00:20:56Z`
- Duración: aproximadamente 12 minutos; se detuvo al ser concluyente
- Saturación del generador: no observada antes de detenerlo

Durante la ejecución se registraron **283 respuestas `202/PENDING`** por
vencer la ventana de confirmación durable. El writer continuó procesando y,
tras detener el generador, el store quedó con `6771` claves del prefijo de la
prueba, `0` filas de outbox sin publicar e `integrity_check=ok`. Esto demuestra
recuperación de lo que alcanzó SQLite, pero no convierte la ejecución parcial
en una validación de capacidad ni prueba que no hubiera solicitudes aún en
tránsito al abortar el generador.

El log del writer mostró pausas de hasta aproximadamente 1.25 s y después
lotes acumulados de hasta 20 comandos. Mosquitto no reportó desconexiones ni
errores equivalentes. El hallazgo llevó a corregir el diseño: el publicador de
eventos actualizaba `published_at` desde una segunda conexión SQLite, en
paralelo con el hilo dueño de las escrituras. Esa doble ruta de escritura
rompía la garantía de single-writer y podía introducir contención durante el
soak.

## Conclusión

10/s **no queda demostrado como capacidad productiva** con este resultado.
Permanece como límite provisional de admisión para reducir el riesgo operativo,
no como SLO ni como capacidad sostenible. Debe repetirse la ejecución contra
el SHA que contiene la corrección de single-writer y completar una hora más
cinco minutos de drenado antes de elevar el perfil.
