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
import hashlib
import io
import tarfile
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
    "ixmati-store-migrate",
]

SYSTEMD_UNITS = [
    "ixmati-cache-server.service",
    "ixmati-api.service",
    "ixmati-writer@.service",
    "ixmati-projector.service",
    "ixmati-litestream-file.service",
    "ixmati-litestream-s3.service",
]

# orden de arranque: cache-server debe estar listo antes que writer/api/projector
# (todos hablan con el por socket Unix, dueño único de Redb desde DEC-0037)
SERVICE_START_ORDER = [
    "mosquitto",
    "ixmati-cache-server",
    "ixmati-writer@default",
    "ixmati-api",
    "ixmati-projector",
    "ixmati-litestream-file",
]

LITESTREAM_VERSION = "0.5.16"
LITESTREAM_INSTALL_PATH = Path("/usr/local/lib/ixmati/litestream")
LITESTREAM_CONFIG_DIR = Path("/etc/ixmati")
LITESTREAM_FILE_CONFIG = LITESTREAM_CONFIG_DIR / "litestream-file.yml"
LITESTREAM_S3_CONFIG = LITESTREAM_CONFIG_DIR / "litestream-s3.yml"
LITESTREAM_ENV = LITESTREAM_CONFIG_DIR / "litestream.env"
LITESTREAM_BACKUP_DIR = Path(
    os.environ.get("IXMATI_LITESTREAM_BACKUP_DIR", "/var/lib/ixmati/backups")
)
LITESTREAM_META_DIR = Path("/var/lib/ixmati/litestream-meta")

# Pinned upstream release artifacts. The installer verifies the archive before
# extracting it; a moving "latest" URL is deliberately never used.
LITESTREAM_RELEASES = {
    "amd64": (
        "x86_64",
        "9e29112380a942e4a62ee07773684396cb8b308dc4d67e130bef41f75e937f0a",
    ),
    "arm64": (
        "arm64",
        "678022e4103145302598e35d37f8718392d42e153feeb1e2d4a64dd0cd3aaf10",
    ),
}

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


def install_litestream() -> None:
    """Install the pinned Litestream binary used by native systemd units."""
    override = os.environ.get("IXMATI_LITESTREAM_BIN", "").strip()
    if override:
        candidate = Path(override)
        if not candidate.is_file() or not os.access(candidate, os.X_OK):
            die(f"IXMATI_LITESTREAM_BIN no es ejecutable: {candidate}")
        LITESTREAM_INSTALL_PATH.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(candidate, LITESTREAM_INSTALL_PATH)
        LITESTREAM_INSTALL_PATH.chmod(0o755)
        ok(f"Litestream instalado desde override: {candidate}")
        return

    if LITESTREAM_INSTALL_PATH.is_file():
        ok(f"Litestream ya instalado: {LITESTREAM_INSTALL_PATH}")
        return

    arch = detect_arch()
    upstream_arch, expected_sha = LITESTREAM_RELEASES[arch]
    filename = f"litestream-{LITESTREAM_VERSION}-linux-{upstream_arch}.tar.gz"
    url = (
        "https://github.com/benbjohnson/litestream/releases/download/"
        f"v{LITESTREAM_VERSION}/{filename}"
    )
    log(f"instalando Litestream v{LITESTREAM_VERSION} ({arch})...")
    try:
        with urllib.request.urlopen(url, timeout=120) as response:
            archive = response.read()
    except (urllib.error.URLError, OSError) as exc:
        die(f"no se pudo descargar Litestream desde {url}: {exc}")

    actual_sha = hashlib.sha256(archive).hexdigest()
    if actual_sha != expected_sha:
        die(
            "checksum de Litestream no coincide: "
            f"esperado {expected_sha}, recibido {actual_sha}"
        )

    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as package:
        binary = next(
            (
                member
                for member in package.getmembers()
                if member.name == "litestream" and member.isfile()
            ),
            None,
        )
        if binary is None:
            die("el archivo de Litestream no contiene el binario esperado")
        extracted = package.extractfile(binary)
        if extracted is None:
            die("no se pudo extraer el binario de Litestream")
        content = extracted.read()

    LITESTREAM_INSTALL_PATH.parent.mkdir(parents=True, exist_ok=True)
    temporary = LITESTREAM_INSTALL_PATH.with_suffix(".new")
    temporary.write_bytes(content)
    temporary.chmod(0o755)
    temporary.replace(LITESTREAM_INSTALL_PATH)
    ok(f"Litestream v{LITESTREAM_VERSION} → {LITESTREAM_INSTALL_PATH}")


def _write_if_missing(path: Path, content: str, mode: int = 0o640) -> None:
    if path.exists():
        warn(f"{path.name} ya existe en {path}, se conserva")
        return
    path.write_text(content)
    path.chmod(mode)
    try:
        shutil.chown(path, user="root", group="ixmati")
    except (LookupError, OSError) as exc:
        die(f"no se pudo proteger {path}: {exc}")
    ok(f"{path.name} → {path}")


def install_litestream_config() -> None:
    """Create native local replication and optional S3 replication configs."""
    LITESTREAM_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    LITESTREAM_BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    (LITESTREAM_META_DIR / "file").mkdir(parents=True, exist_ok=True)
    (LITESTREAM_META_DIR / "s3").mkdir(parents=True, exist_ok=True)
    try:
        shutil.chown(LITESTREAM_BACKUP_DIR, user="ixmati", group="ixmati")
        shutil.chown(LITESTREAM_META_DIR, user="ixmati", group="ixmati")
        shutil.chown(LITESTREAM_META_DIR / "file", user="ixmati", group="ixmati")
        shutil.chown(LITESTREAM_META_DIR / "s3", user="ixmati", group="ixmati")
    except (LookupError, OSError) as exc:
        die(f"no se pudo proteger {LITESTREAM_BACKUP_DIR}: {exc}")

    local_config = f"""# Generated by Ixmati; edit only through the operator config.
sync-interval: 1s
snapshot:
  retention: 168h
dbs:
  - dir: /var/lib/ixmati/stores
    pattern: \"*.db\"
    watch: true
    meta-dir: /var/lib/ixmati/litestream-meta/file
    replica:
      path: {LITESTREAM_BACKUP_DIR}
"""
    _write_if_missing(LITESTREAM_FILE_CONFIG, local_config)

    bucket = os.environ.get(
        "IXMATI_LITESTREAM_S3_BUCKET", os.environ.get("LITESTREAM_S3_BUCKET", "")
    ).strip()
    if not bucket:
        warn(
            "IXMATI_LITESTREAM_S3_BUCKET no está configurado; "
            "la réplica S3 queda deshabilitada"
        )
        return

    prefix = os.environ.get("IXMATI_LITESTREAM_S3_PREFIX", "ixmati").strip()
    region = os.environ.get("IXMATI_LITESTREAM_S3_REGION", "us-east-1").strip()
    endpoint = os.environ.get("IXMATI_LITESTREAM_S3_ENDPOINT", "").strip()
    endpoint_line = f"      endpoint: {endpoint}\n" if endpoint else ""
    s3_config = f"""# Generated by Ixmati; credentials are loaded from litestream.env.
sync-interval: 1s
snapshot:
  retention: 168h
dbs:
  - dir: /var/lib/ixmati/stores
    pattern: \"*.db\"
    watch: true
    meta-dir: /var/lib/ixmati/litestream-meta/s3
    replica:
      url: s3://{bucket}/{prefix}
      region: {region}
{endpoint_line}"""
    _write_if_missing(LITESTREAM_S3_CONFIG, s3_config)

    env_lines = []
    for name in (
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
    ):
        value = os.environ.get(name, "").strip()
        if value:
            if any(char in value for char in "\r\n"):
                die(f"{name} no puede contener saltos de línea")
            # EnvironmentFile is parsed by systemd, not by a shell.  Shell
            # quoting (shlex.quote) is not valid for every credential (for
            # example a value containing a single quote), so emit a systemd
            # double-quoted value with only the required escapes.
            escaped = value.replace("\\", "\\\\").replace('"', '\\"')
            env_lines.append(f'{name}="{escaped}"')
    if not LITESTREAM_ENV.exists():
        _write_if_missing(LITESTREAM_ENV, "\n".join(env_lines) + "\n")
    else:
        warn(f"{LITESTREAM_ENV.name} ya existe en {LITESTREAM_ENV}, se conserva")
    ok("réplica S3 configurada; habilitar ixmati-litestream-s3.service")


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

    # API credentials are deployment state, not an artifact default. Write
    # them only when the operator explicitly supplied IXMATI_API_KEYS and
    # never overwrite the file on reinstall. The service runs as ixmati, so
    # root:ixmati 0640 lets it read the secret without making it public.
    api_env = etc_ixmati / "ixmati.env"
    if api_env.exists():
        warn(f"ixmati.env ya existe en {api_env}, se conserva")
    else:
        api_keys = os.environ.get("IXMATI_API_KEYS", "").strip()
        if not api_keys:
            warn("IXMATI_API_KEYS no está configurado; la API quedará cerrada")
        elif any(char in api_keys for char in "\r\n"):
            die("IXMATI_API_KEYS no puede contener saltos de línea")
        else:
            escaped = api_keys.replace("\\", "\\\\").replace('"', '\\"')
            api_env.write_text(f'IXMATI_API_KEYS="{escaped}"\n')
            api_env.chmod(0o640)
            try:
                shutil.chown(api_env, user="root", group="ixmati")
            except (LookupError, OSError) as exc:
                die(f"no se pudo proteger {api_env}: {exc}")
            ok(f"credenciales API → {api_env} (0640 root:ixmati)")


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


def configured_services() -> list[str]:
    services = list(SERVICE_START_ORDER)
    if LITESTREAM_S3_CONFIG.exists():
        services.append("ixmati-litestream-s3")
    return services


def start_services() -> None:
    log("iniciando servicios...")

    run(["systemctl", "daemon-reload"], quiet=True)

    services = configured_services()
    for svc in services:
        run(["systemctl", "enable", svc], check=False, quiet=True)
        # A plain `start` leaves an already-running process on the previous
        # binary after an upgrade. Restart in dependency order so an
        # idempotent reinstall actually activates the artifact just copied.
        run(["systemctl", "restart", svc], check=False, quiet=True)
        if svc == "ixmati-cache-server":
            wait_for_cache_socket()

    for svc in services:
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
    print("  Configurar credenciales (si no se definieron durante la instalación):")
    print("    sudoedit /etc/ixmati/ixmati.env")
    print('    # IXMATI_API_KEYS="una-clave-larga,otra-clave-en-rotacion"')
    print("    sudo systemctl restart ixmati-api")
    print("")
    print("  Escribir un comando:")
    print('    curl -X POST http://localhost:30000/write \\')
    print('      -H "Authorization: ApiKey <clave-configurada>" \\')
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
def stop_services() -> None:
    log("deteniendo servicios ixmati...")
    for svc in reversed([svc for svc in configured_services() if svc != "mosquitto"]):
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
    if LITESTREAM_INSTALL_PATH.exists():
        LITESTREAM_INSTALL_PATH.unlink()
        ok(str(LITESTREAM_INSTALL_PATH))


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
    install_litestream()
    install_binaries(base_dir)
    # The credential file is owned by root:ixmati, so the service account must
    # exist before install_config writes and protects it.
    create_user()
    install_config(base_dir)
    install_litestream_config()
    configure_mosquitto(base_dir)
    install_systemd_units(base_dir)
    create_directories()
    start_services()
    verify_health()
    show_final_message()


if __name__ == "__main__":
    main()
