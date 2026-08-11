-- helpers/wrk/write_committed.lua — variante de write.lua con
-- ack_mode: "committed" (DEC-0055/Parte 3), para medir latencia real
-- end-to-end (la API solo responde tras confirmar el commit real vía
-- wait_for_commit en rest.rs, no al encolar en MQTT como con "accepted").
--
-- Uso (concurrencia baja a propósito — con el rate-limiter recalibrado en
-- 40/s, DEC-0054, una concurrencia alta solo genera más 429 sin aportar
-- más muestras de latencia real):
--   wrk -t2 -c10 -d30s -s helpers/wrk/write_committed.lua http://localhost:30000/write

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
    '{"op":"upsert","store":"default","entity":"load","key":"k%d-%d-%d","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"wrkc-%d-%d-%d","ack_mode":"committed","payload":{"seq":%d}}',
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
  io.write("\n--- resumen wrk (ack_mode=committed, latencia end-to-end real) ---\n")
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
