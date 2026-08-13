# helpers/just/installer.just — tareas del instalador nativo (systemd)

# empaqueta dist/ y valida su contenido
[group('installer')]
installer-dist:
    @make dist
    @make dist-checksums
    @make dist-validate

# construye dist/ y valida el instalador dentro de un contenedor Debian
# con systemd real (requiere podman --privileged)
[group('installer')]
installer-test:
    @{{repo_root}}/helpers/shell/test_installer_debian.sh

# verifica la réplica y el restore usando un destino local montado y file://
[group('installer')]
litestream-local:
    @{{repo_root}}/helpers/shell/test_litestream_local.sh
