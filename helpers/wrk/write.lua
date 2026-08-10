-- helpers/wrk/write.lua — script de carga real para POST /write (DEC-0046, TASK-VAL-0011)
--
-- A diferencia de test_stack_validation.sh (curl secuencial/paralelo en
-- subshells de bash, mide el harness, no el servidor — ver DEC-0042), wrk
-- es un solo proceso epoll sin overhead de fork/exec por request.
--
-- Uso (dentro de un contenedor Debian con `apt-get install wrk`):
--   wrk -t4 -c50 -d60s -s helpers/wrk/write.lua http://localhost:30000/write
--
-- IMPORTANTE: correr wrk en el MISMO host/contenedor que ixmati-api hace que
-- compitan por la misma CPU — para un número de capacidad limpio, wrk debe
-- correr en una máquina/contenedor separado del target (ver DEC-0046).
--
-- El default de MAX_WRITES_PER_WINDOW (1000/s por store) va a rechazar la
-- mayoría de las requests con 429 a la concurrencia que wrk puede generar;
-- para medir el techo real del código (no del rate-limiter), subir
-- MAX_WRITES_PER_WINDOW vía override de systemd antes de correr.

wrk.method = "POST"
wrk.headers["Authorization"] = "ApiKey ix-default-key"
wrk.headers["Content-Type"] = "application/json"

local setup_counter = 0

function setup(thread)
  thread:set("id", setup_counter)
  setup_counter = setup_counter + 1
end

local id = 0
local counter = 0
local run_nonce = 0

function init(args)
  id = wrk.thread:get("id")
  math.randomseed(os.time() * 1000 + id)
  run_nonce = math.random(1, 999999999)
end

request = function()
  counter = counter + 1
  local body = string.format(
    '{"op":"upsert","store":"default","entity":"load","key":"k%d-%d-%d","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"wrk-%d-%d-%d","ack_mode":"accepted","payload":{"seq":%d}}',
    run_nonce, id, counter, run_nonce, id, counter, counter
  )
  return wrk.format(nil, nil, nil, body)
end

response = function(status, headers, body)
  if status ~= 200 then
    errors_by_status = errors_by_status or {}
    errors_by_status[status] = (errors_by_status[status] or 0) + 1
  end
end

done = function(summary, latency, requests)
  io.write("\n--- resumen wrk ---\n")
  io.write(string.format("requests=%d errors_connect=%d errors_read=%d errors_write=%d errors_status=%d errors_timeout=%d\n",
    summary.requests, summary.errors.connect, summary.errors.read, summary.errors.write, summary.errors.status, summary.errors.timeout))
  io.write(string.format("latency p50=%.2fms p90=%.2fms p99=%.2fms max=%.2fms\n",
    latency:percentile(50)/1000, latency:percentile(90)/1000, latency:percentile(99)/1000, latency.max/1000))
  if errors_by_status then
    for status, count in pairs(errors_by_status) do
      io.write(string.format("http_status_%d=%d\n", status, count))
    end
  end
end
