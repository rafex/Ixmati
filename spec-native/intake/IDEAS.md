# Intake

Ideas pendientes de triage. No son tareas ejecutables ni aparecen en el
tablero de entrega hasta que se promueven a una spec y sus tareas derivadas.

## 2026-08-04: Installer .deb tipo PostgreSQL con repositorio APT

- owner: team
- priority: p3
- labels: backlog, packaging, debian, installer

Empaquetar Ixmati como .deb Debian/Ubuntu. Capa sobre installer.py existente con:
- debian/control con Depends: mosquitto (>= 2.0)
- postinst/preinst/prerm/postrm para lifecycle systemd
- ixmati-create-store <name> helper
- Repositorio APT con CI/CD y firmas GPG
Criterio activacion: >= 3 equipos pidiendo apt install + core estable.
