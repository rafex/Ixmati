#!/usr/bin/env uv run
# installer.py — Ixmati installer for Linux servers
#
# Modo online:  uv run installer.py --version 0.1.0
# Modo offline: uv run installer.py --offline --tarball ixmati-0.1.0-linux-amd64.tar.gz
# 
# Acciones:
#  1. Descarga el tarball desde GitHub Releases (modo online)
#  2. Extrae binarios a /usr/local/bin/
#  3. Crea usuario ixmati e instala systemd units
#  4. Configura directorios de datos y Mosquitto
#  5. Habilita e inicia servicios

import hashlib
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Optional

# --version --version
REPO = "rafex/Ixmati"
DEFAULT_VERSION = "0.1.0"

BINARIES = [
    "ixmati-api",
    "ixmati-writer",
    "ixmati-projector",
    "ixmati-supervisor",
    "ixmati-reconciler",
]


def log(msg: str) -> None:
    print(f"[installer] {msg}")


def ok(msg: str) -> None:
    print(f"  \033[32m✓\033[0m {msg}")


def warn(msg: str) -> None:
    print(f"  \033[33m⚠\033[0m {msg}")


def die(msg: str) -> None:
    print(f"\033[31m[ERROR]\033[0m {msg}", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess:
    log(f"$ {shlex.join(cmd)}")
    return subprocess.run(cmd, check=check)


def detect_arch() -> str:
    """Detecta arquitectura: amd64 o arm64."""
    machine = platform.machine()
    if machine in ("x86_64", "AMD64"):
        return "amd64"
    if machine in ("aarch64", "arm64"):
        return "arm64"
    die(f"Arquitectura no soportada: {machine}")


def download_tarball(version: str, arch: str, dest: Path) -> Path:
    """Descarga el tarball desde GitHub Releases."""
    file_name = f"ixmati-{version}-linux-{arch}.tar.gz"
    url = f"https://github.com/{REPO}/releases/download/v{version}/{file_name}"

    log(f"descargando {url}")
    result = run(
        ["curl", "-sSL", "-o", str(dest / file_name), url],
        check=False,
    )
    if result.returncode != 0:
        die(f"no se pudo descargar {url}")

    ok(f"{file_name} descargado")
    return dest / file_name


def verify_checksum(tarball: Path, version: str) -> None:
    """Verifica SHA256 del tarball."""
    sha_url = (
        f"https://github.com/{REPO}/releases/download/v{version}/{tarball.name}.sha256"
    )
    result = subprocess.run(
        ["curl", "-sSL", sha_url],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        warn("no se pudo verificar checksum (saltando)")
        return

    expected_hash = result.stdout.strip().split()[0]
    actual_hash = hashlib.sha256(tarball.read_bytes()).hexdigest()

    if actual_hash == expected_hash:
        ok(f"checksum verificado: {actual_hash[:16]}...")
    else:
        die(f"checksum no coincide: esperado={expected_hash[:16]}..., actual={actual_hash[:16]}...")


def extract_binaries(tarball: Path, prefix: Path) -> None:
    """Extrae binarios del tarball a prefix/bin/."""
    log(f"extrayendo a {prefix}/bin/")
    (prefix / "bin").mkdir(parents=True, exist_ok=True)

    with tarfile.open(tarball, "r:gz") as tar:
        for member in tar.getmembers():
            name = Path(member.name).name
            if name in BINARIES:
                tar.extract(member, prefix / "bin")
                (prefix / "bin" / name).chmod(0o755)
                ok(f"  {name}")
            elif member.name.endswith(("stores.example.toml", "projections.example.toml")):
                config_dir = Path("/etc/ixmati")
                config_dir.mkdir(parents=True, exist_ok=True)
                target = config_dir / Path(member.name).name
                tar.extract(member, config_dir)
                ok(f"  config: {target}")


def create_user(username: str = "ixmati") -> None:
    """Crea el usuario de servicio si no existe."""
    result = subprocess.run(["id", username], check=False, capture_output=True)
    if result.returncode == 0:
        ok(f"usuario {username} ya existe")
        return

    log(f"creando usuario {username}")
    run(["useradd", "--system", "--no-create-home", "--shell", "/usr/sbin/nologin", username])
    ok(f"usuario {username} creado")


def create_directories() -> None:
    """Crea directorios de datos con permisos correctos."""
    dirs = [
        ("/var/lib/ixmati/stores", "ixmati", "ixmati"),
        ("/var/lib/ixmati/cache", "ixmati", "ixmati"),
        ("/var/log/ixmati", "ixmati", "ixmati"),
        ("/etc/ixmati", "root", "root"),
    ]
    for path, owner, group in dirs:
        Path(path).mkdir(parents=True, exist_ok=True)
        run(["chown", f"{owner}:{group}", path])
        ok(f"directorio {path}")


def install_quadlet_units() -> None:
    """Instala unidades quadlet de systemd."""
    quadlet_dir = Path.home() / ".config/containers/systemd"
    units_dir = Path("/etc/ixmati/quadlet")
    units_dir.mkdir(parents=True, exist_ok=True)

    # Copia unidades quadlet de referencia
    repo_quadlet = Path("/tmp/ixmati-quadlet")
    if repo_quadlet.exists():
        for unit in repo_quadlet.glob("ixmati-*"):
            shutil.copy(unit, units_dir)

    # Si estamos en el repo extraído
    tarball_quadlet = Path(".").absolute()
    for unit in tarball_quadlet.glob("containers/quadlet/ixmati-*"):
        shutil.copy(unit, units_dir)
        ok(f"  quadlet: {unit.name}")

    run(["systemctl", "--user", "daemon-reload"], check=False)
    ok("systemd daemon-reload")


def enable_services() -> None:
    """Habilita servicios systemd de Mosquitto y Ixmati."""
    services = ["mosquitto"]
    for svc in services:
        result = subprocess.run(["systemctl", "enable", svc], check=False, capture_output=True)
        if result.returncode == 0:
            ok(f"  {svc}.service enabled")
        else:
            warn(f"  {svc}.service no disponible")

    # Quadlet units (rootless, --user)
    quadlet_units = ["ixmati-api", "ixmati-mosquitto", "ixmati-projector"]
    for unit in quadlet_units:
        run(
            ["systemctl", "--user", "enable", f"{unit}.service"],
            check=False,
        )


def install_mosquitto() -> None:
    """Instala y configura Mosquitto si no existe."""
    result = subprocess.run(["which", "mosquitto"], check=False, capture_output=True, text=True)
    if result.returncode == 0:
        ok(f"Mosquitto ya instalado: {result.stdout.strip()}")
        return

    log("instalando Mosquitto...")
    run(["apt-get", "update", "-qq"])
    run(["apt-get", "install", "-y", "-qq", "mosquitto", "mosquitto-clients"])

    config = """/etc/mosquitto/conf.d/ixmati.conf
listener 1883
protocol mqtt
persistence true
persistence_location /var/lib/mosquitto/
allow_anonymous false
log_dest syslog
"""
    Path("/etc/mosquitto/conf.d/ixmati.conf").write_text(config)
    ok("Mosquitto configurado")


def show_next_steps(prefix: Path) -> None:
    """Muestra instrucciones post-instalación."""
    print("")
    print("=" * 60)
    print("  Ixmati instalado correctamente")
    print("=" * 60)
    print("")
    print("  Binarios:")
    for binary in BINARIES:
        path = prefix / "bin" / binary
        if path.exists():
            print(f"    {path}")
    print("")
    print("  Configuración:")
    print(f"    /etc/ixmati/stores.example.toml")
    print(f"    /etc/ixmati/projections.example.toml")
    print("")
    print("  Siguientes pasos:")
    print(f"    1. cp /etc/ixmati/stores.example.toml /etc/ixmati/stores.toml")
    print(f"    2. Editar /etc/ixmati/stores.toml con tus stores")
    print(f"    3. systemctl --user start ixmati-mosquitto.service")
    print(f"    4. systemctl --user start ixmati-api.service")
    print(f"    5. systemctl --user start ixmati-writer@pedidos.service")
    print(f"    6. curl http://localhost:30000/health")
    print("")


def main() -> None:
    version = os.environ.get("IXMATI_VERSION", DEFAULT_VERSION)
    prefix = Path(os.environ.get("IXMATI_PREFIX", "/usr/local"))
    offline = "--offline" in sys.argv
    tarball_arg: Optional[str] = None
    no_systemd = "--no-systemd" in sys.argv
    no_mosquitto = "--no-mosquitto" in sys.argv

    for i, arg in enumerate(sys.argv[1:], 1):
        if arg == "--version" and i + 1 <= len(sys.argv):
            version = sys.argv[i + 1]
        elif arg == "--prefix" and i + 1 <= len(sys.argv):
            prefix = Path(sys.argv[i + 1])
        elif arg == "--tarball" and i + 1 <= len(sys.argv):
            tarball_arg = sys.argv[i + 1]

    if os.geteuid() != 0:
        die("debes ejecutar como root (usa sudo)")

    arch = detect_arch()
    log(f"instalando Ixmati v{version} para linux/{arch}")

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)

        if offline and tarball_arg:
            tarball = Path(tarball_arg)
            if not tarball.exists():
                die(f"tarball no encontrado: {tarball}")
            ok(f"modo offline: {tarball}")
        elif offline:
            candidates = list(Path(".").glob("ixmati-*.tar.gz"))
            if not candidates:
                die("modo offline sin --tarball y sin .tar.gz en directorio actual")
            tarball = candidates[0]
            ok(f"modo offline: {tarball}")
        else:
            tarball = download_tarball(version, arch, tmp_path)
            verify_checksum(tarball, version)

        extract_binaries(tarball, prefix)
        create_user()
        create_directories()

        if not no_mosquitto:
            install_mosquitto()

        if not no_systemd:
            install_quadlet_units()
            enable_services()

        show_next_steps(prefix)


if __name__ == "__main__":
    main()
