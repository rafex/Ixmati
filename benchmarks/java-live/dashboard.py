#!/usr/bin/env python3
"""Small dependency-free dashboard for the containerized Java live demo."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.request import Request, urlopen
import json
import os
import re
import time

SNAPSHOTS = Path(os.getenv("SNAPSHOT_DIR", "/snapshots"))
DIRECT_DB = Path(os.getenv("DIRECT_DB", "/direct-data/demo.sqlite"))
API_HEALTH = os.getenv("IXMATI_HEALTH_URL", "http://api:30000/health")
WRITER_METRICS = os.getenv("IXMATI_WRITER_METRICS_URL", "http://writer:30300/metrics")
START = time.time()

def fetch(url):
    try:
        with urlopen(Request(url, headers={"Accept": "text/plain"}), timeout=1.5) as response:
            return response.read().decode()
    except Exception:
        return ""

def metric(text, name):
    values = []
    for line in text.splitlines():
        if line.startswith(name) and not line.startswith("#"):
            match = re.search(r"\s(-?[0-9]+(?:\.[0-9]+)?)\s*$", line)
            if match:
                values.append(float(match.group(1)))
    return sum(values) if values else 0

def snapshots():
    rows = []
    for path in sorted(SNAPSHOTS.glob("*.jsonl")):
        try:
            lines = path.read_text().splitlines()
            if lines:
                rows.append(json.loads(lines[-1]))
        except (OSError, json.JSONDecodeError):
            continue
    return rows

def state():
    rows = snapshots()
    groups = {}
    for row in rows:
        groups.setdefault(row.get("mode", "unknown"), []).append(row)
    result = {"ts": time.time(), "elapsed_seconds": round(time.time() - START, 1), "clients": rows, "sides": {}}
    for side, values in groups.items():
        def total(key): return sum(float(v.get(key, 0)) for v in values)
        def max_value(key): return max((float(v.get(key, 0)) for v in values), default=0)
        result["sides"][side] = {
            "clients": len(values), "writes_sent": total("writes_sent"), "writes_committed": total("writes_committed"),
            "pending": total("pending"), "write_errors": total("write_errors"), "sqlite_busy": total("sqlite_busy"),
            "reads": total("reads"), "read_hits": total("read_hits"), "read_errors": total("read_errors"),
            "p50_ms": max_value("p50_ms"), "p95_ms": max_value("p95_ms"), "p99_ms": max_value("p99_ms"),
            "total_writes": total("total_writes"), "total_committed": total("total_committed"),
            "total_pending": total("total_pending"), "total_write_errors": total("total_write_errors"),
            "total_reads": total("total_reads"), "total_read_hits": total("total_read_hits"),
            "total_read_errors": total("total_read_errors"), "total_sqlite_busy": total("total_sqlite_busy"),
        }
    metrics = fetch(WRITER_METRICS)
    result["ixmati_metrics"] = {
        "consumer_queue_depth": metric(metrics, "ixmati_consumer_queue_depth"),
        "cache_sync_queue_depth": metric(metrics, "ixmati_cache_sync_queue_depth"),
        "outbox_published": metric(metrics, "ixmati_outbox_published_total"),
        "mqtt_ack_failures": metric(metrics, "ixmati_mqtt_ack_failures_total"),
        "last_batch_commit_unix_seconds": metric(metrics, "ixmati_last_batch_commit_unix_seconds"),
    }
    wal = DIRECT_DB.with_name(DIRECT_DB.name + "-wal")
    result["direct_files"] = {"db_bytes": DIRECT_DB.stat().st_size if DIRECT_DB.exists() else 0, "wal_bytes": wal.stat().st_size if wal.exists() else 0}
    result["services"] = {"api_health": bool(fetch(API_HEALTH))}
    return result

HTML = """<!doctype html><html><head><meta charset=utf-8><title>Ixmati Java live</title>
<style>body{font:14px system-ui;background:#10151c;color:#e8eef5;margin:24px}h1{margin:0 0 4px}
.warning{color:#ffd166;background:#2b2515;padding:10px;border-radius:6px;margin:14px 0}
.grid{display:grid;grid-template-columns:1fr 1fr;gap:16px}.card{background:#18212b;padding:16px;border-radius:8px}
table{width:100%;border-collapse:collapse}td{padding:5px;border-bottom:1px solid #2b3642}.num{text-align:right;font-variant-numeric:tabular-nums}
pre{white-space:pre-wrap;color:#b9c7d5}</style></head>
<body><h1>SQLite directo vs Ixmati</h1><div class=warning>Demo contenerizada concurrente: ambos lados comparten CPU, memoria y filesystem. No es un benchmark aislado de capacidad.</div>
<div id=app>cargando...</div><script>
const fields=[['writes_sent','Escrituras ventana'],['writes_committed','Confirmadas'],['pending','PENDING'],['write_errors','Errores escritura'],['sqlite_busy','SQLITE_BUSY'],['reads','Lecturas'],['read_hits','Lecturas encontradas'],['p50_ms','p50 ms'],['p95_ms','p95 ms'],['p99_ms','p99 ms']];
function num(x){return Number(x||0).toLocaleString(undefined,{maximumFractionDigits:2})}
function card(name,s){if(!s)return '<section class=card><h2>'+name+'</h2><p>sin snapshots</p></section>';let rows=fields.map(function(f){return '<tr><td>'+f[1]+'</td><td class=num>'+num(s[f[0]])+'</td></tr>'}).join('');return '<section class=card><h2>'+name+'</h2><table>'+rows+'</table><p>Total escrituras: '+num(s.total_writes)+' · total lecturas: '+num(s.total_reads)+'</p></section>'}
async function refresh(){let d=await fetch('/state').then(function(r){return r.json()});document.getElementById('app').innerHTML='<p>Transcurrido: '+num(d.elapsed_seconds)+' s · clientes: '+d.clients.length+'</p><div class=grid>'+card('SQLite directo',d.sides.direct)+card('Ixmati + SQLite',d.sides.ixmati)+'</div><section class=card><h2>Operación Ixmati</h2><pre>'+JSON.stringify(d.ixmati_metrics,null,2)+'</pre><p>API health: '+(d.services.api_health?'OK':'sin respuesta')+' · SQLite directo: '+num(d.direct_files.db_bytes)+' bytes · WAL: '+num(d.direct_files.wal_bytes)+' bytes</p></section>'}
refresh();setInterval(refresh,1000);
</script></body></html>"""

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/state":
            body = json.dumps(state()).encode()
            self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body); return
        body = HTML.encode()
        self.send_response(200); self.send_header("Content-Type", "text/html; charset=utf-8"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *_): pass

if __name__ == "__main__":
    ThreadingHTTPServer(("0.0.0.0", int(os.getenv("PORT", "8080"))), Handler).serve_forever()
