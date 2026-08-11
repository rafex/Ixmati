#!/usr/bin/env python3
"""installer.py — Ixmati installer (native systemd, no containers)

Modo offline (desde el tar.gz):
    sudo ./install.sh

Modo online:
    curl -sSL https://raw.githubusercontent.com/rafex/Ixmati/main/scripts/install.sh | sudo bash

Desinstalar:
    sudo ./install.sh --uninstall            # detiene servicios, quita binarios/config
    sudo ./install.sh --uninstall --purge    # además borra datos y el usuario ixmati
"""

import os
import platform
import shlex
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import NoReturn

BINARIES = [
    "ixmati-cache-server",
    "ixmati-api",
    "ixmati-writer",
    "ixmati-projector",
    "ixmati-supervisor",
    "ixmati-reconciler",
]

SYSTEMD_UNITS = [
    "ixmati-cache-server.service",
    "ixmati-api.service",
    "ixmati-writer@.service",
    "ixmati-projector.service",
]

# orden de arranque: cache-server debe estar listo antes que writer/api/projector
# (todos hablan con el por socket Unix, dueño único de Redb desde DEC-0037)
SERVICE_START_ORDER = [
    "mosquitto",
    "ixmati-cache-server",
    "ixmati-writer@default",
    "ixmati-api",
    "ixmati-projector",
]

CONFIG_FILES = [
    "stores.toml",
    "projections.toml",
]


def log(msg: str) -> None:
    print(f"  \033[34m[ixmati]\033[0m {msg}")


def ok(msg: str) -> None:
    print(f"    \033[32m✓\033[0m {msg}")


def warn(msg: str) -> None:
    print(f"    \033[33m⚠\033[0m {msg}")


def die(msg: str) -> NoReturn:
    print(f"  \033[31m[ERROR]\033[0m {msg}", file=sys.stderr)
    raise SystemExit(1)  # unreachable; satisfies type checker for die()


def run(cmd: list[str], check: bool = True, quiet: bool = False) -> subprocess.CompletedProcess:
    if not quiet:
        log(f"$ {shlex.join(cmd)}")
    return subprocess.run(cmd, check=check)


def detect_arch() -> str:
    machine = platform.machine()
    if machine in ("x86_64", "AMD64"):
        return "amd64"
    if machine in ("aarch64", "arm64"):
        return "arm64"
    die(f"Arquitectura no soportada: {machine}")


def detect_os() -> str:
    system = platform.system()
    if system != "Linux":
        die(f"Sistema no soportado: {system}. Ixmati requiere Linux.")
    return system


def find_base_dir() -> Path:
    script = Path(__file__).resolve()
    candidate = script.parent
    if (candidate / "install.sh").exists() or (candidate / "bin").exists():
        return candidate

    for path in script.parents:
        if (path / "bin").exists():
            return path

    die("no se encontró el directorio de instalación (busca bin/ en padres de installer.py)")


def install_mosquitto() -> None:
    log("verificando Mosquitto...")
    result = subprocess.run(["which", "mosquitto"], check=False, capture_output=True)
    if result.returncode == 0:
        ok("Mosquitto ya instalado")
        return

    log("instalando Mosquitto...")
    if shutil.which("apt-get"):
        run(["apt-get", "update", "-qq"])
        run(["apt-get", "install", "-y", "-qq", "mosquitto", "mosquitto-clients"])
    elif shutil.which("dnf"):
        run(["dnf", "install", "-y", "mosquitto"])
    elif shutil.which("yum"):
        run(["yum", "install", "-y", "mosquitto"])
    else:
        die("no se detectó apt-get, dnf ni yum. Instala Mosquitto manualmente.")
    ok("Mosquitto instalado")


MOSQUITTO_MARKER = "# ixmati-managed"


def configure_mosquitto(base_dir: Path) -> None:
    log("configurando Mosquitto...")
    conf_src = base_dir / "config" / "mosquitto" / "mosquitto.conf"
    conf_dst = Path("/etc/mosquitto/mosquitto.conf")
    conf_backup = Path("/etc/mosquitto/mosquitto.conf.pre-ixmati")

    # limpia un fragmento conf.d de una instalación previa (rota: el paquete
    # Debian ya define persistence/persistence_location y mosquitto rechaza
    # como fatal un conf.d que las duplique)
    legacy_fragment = Path("/etc/mosquitto/conf.d/ixmati.conf")
    if legacy_fragment.exists():
        legacy_fragment.unlink()
        warn(f"eliminado fragmento obsoleto: {legacy_fragment}")

    if not conf_src.exists():
        warn("mosquitto.conf no encontrado en artefacto")
        return

    if conf_dst.exists() and MOSQUITTO_MARKER in conf_dst.read_text():
        warn(f"{conf_dst} ya gestionado por ixmati, se conserva")
        return

    if conf_dst.exists() and not conf_backup.exists():
        shutil.copy2(conf_dst, conf_backup)
        ok(f"mosquitto.conf original respaldado → {conf_backup}")

    shutil.copy2(conf_src, conf_dst)
    ok(f"mosquitto.conf → {conf_dst} (reemplaza el archivo completo)")


def install_binaries(base_dir: Path) -> None:
    log("instalando binarios...")
    bin_dir = Path("/usr/local/bin")
    bin_dir.mkdir(parents=True, exist_ok=True)

    src_bin = base_dir / "bin"
    for binary in BINARIES:
        src = src_bin / binary
        if src.exists():
            dst = bin_dir / binary
            # copia a un temporal + rename atómico: sobrescribir en el sitio
            # con open(dst, "wb") falla con "Text file busy" si el binario
            # anterior sigue corriendo (reinstalación con servicios activos)
            tmp_dst = dst.with_suffix(dst.suffix + ".new")
            shutil.copy2(src, tmp_dst)
            tmp_dst.chmod(0o755)
            tmp_dst.replace(dst)
            ok(binary)
        else:
            warn(f"{binary} no encontrado")


def install_config(base_dir: Path) -> None:
    log("instalando configuración...")
    etc_ixmati = Path("/etc/ixmati")
    etc_ixmati.mkdir(parents=True, exist_ok=True)

    src_config = base_dir / "config"
    for cfg_file in CONFIG_FILES:
        src = src_config / cfg_file
        dst = etc_ixmati / cfg_file
        if not src.exists():
            continue
        if dst.exists():
            warn(f"{cfg_file} ya existe en {dst}, se conserva")
            continue
        shutil.copy2(src, dst)
        ok(f"{cfg_file} → {dst}")

    mosquitto_conf = src_config / "mosquitto" / "mosquitto.conf"
    mosquitto_dst = etc_ixmati / "mosquitto" / "mosquitto.conf"
    if mosquitto_conf.exists():
        if mosquitto_dst.exists():
            warn(f"mosquitto.conf ya existe en {mosquitto_dst}, se conserva")
        else:
            mosquitto_dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(mosquitto_conf, mosquitto_dst)
            ok(f"mosquitto.conf → {mosquitto_dst}")


def install_systemd_units(base_dir: Path) -> None:
    log("instalando unidades systemd...")
    unit_dir = Path("/etc/systemd/system")
    unit_dir.mkdir(parents=True, exist_ok=True)

    src_units = base_dir / "systemd"
    for unit_file in SYSTEMD_UNITS:
        src = src_units / unit_file
        dst = unit_dir / unit_file
        if src.exists():
            shutil.copy2(src, dst)
            ok(unit_file)
        else:
            warn(f"{unit_file} no encontrado")

    run(["systemctl", "daemon-reload"])


def create_user() -> None:
    log("configurando usuario ixmati...")
    result = subprocess.run(["id", "ixmati"], check=False, capture_output=True)
    if result.returncode == 0:
        ok("usuario ixmati ya existe")
    else:
        run(["useradd", "--system", "--no-create-home", "--shell", "/usr/sbin/nologin", "ixmati"])
        ok("usuario ixmati creado")


def create_directories() -> None:
    log("creando directorios de datos...")
    dirs = [
        ("/var/lib/ixmati/stores", "ixmati", "ixmati"),
        ("/var/lib/ixmati/cache", "ixmati", "ixmati"),
        ("/var/log/ixmati", "ixmati", "ixmati"),
    ]
    for path, owner, group in dirs:
        Path(path).mkdir(parents=True, exist_ok=True)
        run(["chown", f"{owner}:{group}", path])
        ok(path)


def wait_for_cache_socket(timeout_s: float = 10.0) -> None:
    socket_path = Path("/var/run/ixmati/cache.sock")
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if socket_path.exists():
            ok(f"cache-server socket listo ({socket_path})")
            return
        time.sleep(0.2)
    warn(f"cache-server socket no apareció tras {timeout_s}s ({socket_path})")


def start_services() -> None:
    log("iniciando servicios...")

    run(["systemctl", "daemon-reload"], quiet=True)

    for svc in SERVICE_START_ORDER:
        run(["systemctl", "enable", svc], check=False, quiet=True)
        # A plain `start` leaves an already-running process on the previous
        # binary after an upgrade. Restart in dependency order so an
        # idempotent reinstall actually activates the artifact just copied.
        run(["systemctl", "restart", svc], check=False, quiet=True)
        if svc == "ixmati-cache-server":
            wait_for_cache_socket()

    for svc in SERVICE_START_ORDER:
        result = subprocess.run(
            ["systemctl", "is-active", svc],
            check=False, capture_output=True, text=True,
        )
        status = result.stdout.strip() if result.returncode == 0 else "inactive"
        icon = "✓" if status == "active" else "✗"
        print(f"    {icon} {svc} → {status}")


def verify_health(
    host: str = "localhost", port: int = 30000, retries: int = 10
) -> bool:
    log("verificando health check...")
    url = f"http://{host}:{port}/health"
    for _attempt in range(retries):
        try:
            with urllib.request.urlopen(url, timeout=2) as resp:
                if resp.status == 200:
                    ok(f"GET {url} → 200")
                    return True
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(1)
    warn(f"GET {url} no respondió 200 tras {retries}s")
    return False


def show_final_message() -> None:
    print("")
    print("=" * 60)
    print("  \033[32mIxmati instalado correctamente\033[0m")
    print("=" * 60)
    print("")
    print("  Health check:")
    print("    curl http://localhost:30000/health")
    print("")
    print("  Escribir un comando:")
    print('    curl -X POST http://localhost:30000/write \\')
    print('      -H "Authorization: ApiKey ix-default-key" \\')
    print('      -H "Content-Type: application/json" \\')
    print('      -d \'{"op":"upsert","store":"default","entity":"test","key":"k1","version":1,')
    print('           "ts":"2026-01-01T00:00:00Z","idempotency_key":"$(uuidgen)",')
    print('           "ack_mode":"committed","payload":{"hello":"world"}}\'')
    print("")
    print("  Logs:")
    print("    journalctl -u ixmati-api -f")
    print("    journalctl -u ixmati-writer@default -f")
    print("")
    print("  Agregar más stores:")
    print("    vim /etc/ixmati/stores.toml")
    print("    systemctl enable ixmati-writer@<nuevo-store>")
    print("    systemctl start ixmati-writer@<nuevo-store>")
    print("")
    print("  Desinstalar:")
    print("    sudo ./install.sh --uninstall            # conserva datos")
    print("    sudo ./install.sh --uninstall --purge    # borra datos también")
    print("")


# servicios propios de ixmati (excluye mosquitto, que es un paquete del sistema)
IXMATI_SERVICES = [svc for svc in SERVICE_START_ORDER if svc != "mosquitto"]


def stop_services() -> None:
    log("deteniendo servicios ixmati...")
    for svc in reversed(IXMATI_SERVICES):
        run(["systemctl", "stop", svc], check=False, quiet=True)
        run(["systemctl", "disable", svc], check=False, quiet=True)
        ok(svc)


def remove_systemd_units() -> None:
    log("quitando unidades systemd...")
    unit_dir = Path("/etc/systemd/system")
    for unit_file in SYSTEMD_UNITS:
        dst = unit_dir / unit_file
        if dst.exists():
            dst.unlink()
            ok(unit_file)
    run(["systemctl", "daemon-reload"], quiet=True)


def remove_binaries() -> None:
    log("quitando binarios...")
    bin_dir = Path("/usr/local/bin")
    for binary in BINARIES:
        dst = bin_dir / binary
        if dst.exists():
            dst.unlink()
            ok(binary)


def remove_config() -> None:
    log("quitando configuración...")
    etc_ixmati = Path("/etc/ixmati")
    if etc_ixmati.exists():
        shutil.rmtree(etc_ixmati)
        ok(str(etc_ixmati))

    conf_dst = Path("/etc/mosquitto/mosquitto.conf")
    conf_backup = Path("/etc/mosquitto/mosquitto.conf.pre-ixmati")
    if conf_backup.exists():
        shutil.move(str(conf_backup), str(conf_dst))
        ok(f"mosquitto.conf original restaurado (desde {conf_backup})")


def purge_data() -> None:
    log("purgando datos y usuario ixmati...")
    for path in ("/var/lib/ixmati", "/var/log/ixmati"):
        p = Path(path)
        if p.exists():
            shutil.rmtree(p)
            ok(path)

    result = subprocess.run(["id", "ixmati"], check=False, capture_output=True)
    if result.returncode == 0:
        run(["userdel", "ixmati"], check=False, quiet=True)
        ok("usuario ixmati eliminado")


def uninstall(purge: bool) -> None:
    if os.geteuid() != 0:
        die("debes ejecutar como root (sudo)")

    log("desinstalando Ixmati...")
    stop_services()
    remove_systemd_units()
    remove_binaries()
    remove_config()
    if purge:
        purge_data()

    print("")
    print("=" * 60)
    suffix = " (con purga de datos)" if purge else ""
    print(f"  \033[32mIxmati desinstalado\033[0m{suffix}")
    print("=" * 60)
    if not purge:
        print("  Datos conservados en /var/lib/ixmati y /var/log/ixmati.")
    print("")


def main() -> None:
    args = sys.argv[1:]
    if "--uninstall" in args:
        uninstall(purge="--purge" in args)
        return

    if os.geteuid() != 0:
        die("debes ejecutar como root (sudo)")

    if platform.system() != "Linux":
        die("Ixmati solo funciona en Linux")

    arch = detect_arch()
    log(f"Ixmati installer — linux/{arch}")
    log("")

    base_dir = find_base_dir()
    ok(f"directorio de instalación: {base_dir}")

    install_mosquitto()
    install_binaries(base_dir)
    install_config(base_dir)
    configure_mosquitto(base_dir)
    install_systemd_units(base_dir)
    create_user()
    create_directories()
    start_services()
    verify_health()
    show_final_message()


if __name__ == "__main__":
    main()
