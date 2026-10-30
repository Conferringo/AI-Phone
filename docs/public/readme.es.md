# AI Phone

[English](../../README.md) · [Todos los idiomas](../../README.md) · Apache-2.0

## Descargas y estado

Los instaladores completos todavía no están disponibles. La configuración sigue el orden recursos de los dispositivos → modelo → espacio de trabajo. Un paquete completo contiene los sistemas Android/Linux, el emulador y los servicios precompilados. El cliente prepara el entorno; el usuario no tiene que buscar imágenes ni compilar un servidor.

Los destinos de compilación son macOS Apple Silicon, Windows x64, Linux x64/ARM64 y Android ARM64. El anfitrión Android requiere root y KVM. La disponibilidad de paquetes y el alcance de las pruebas figuran en el estado de publicación.

## Funciones y escenarios

AI Phone combina Android invitado real, un escritorio Linux ligero y Agentes. Por defecto se ofrece un teléfono y un ordenador, cada uno con su conversación continua. Pueden compartir un Agente o usar modelos distintos.

- Usar aplicaciones como WeChat mediante su pantalla y notificaciones reales, y responder según las instrucciones del usuario. Las restricciones de cuenta y aplicación siguen vigentes.
- Recibir un archivo en el teléfono, transferirlo a Linux, procesar y comprobar una hoja de cálculo y devolver el resultado. Transferencia entre dispositivos y entrega a un contacto se verifican por separado.
- Añadir instrucciones e interrumpir la generación sin perder mensajes ni acciones. Las notificaciones pueden despertar al Agente; la exploración autónoma se pausa por dispositivo.
- Compartir Skills, documentos de conocimiento vinculados y archivos de trabajo. Modelos/API, memoria y conexiones externas pertenecen a los Agentes. Clash se ejecuta dentro de cada invitado; el host usa conexión directa por defecto.

Las acciones se vinculan a observaciones, dispositivo, tarea, época de control e identificador único. Un resultado desconocido se observa antes de repetir. Los modelos pueden interpretar mal las interfaces; los registros no garantizan precisión absoluta.

## Arquitectura y datos

`apps/web`: Next.js App Router, React, Tailwind, shadcn. `apps/client`: Tauri 2. `crates`: Rust/Axum/Tokio. `packages/protocol`: protobuf. PostgreSQL/SQLx conserva el estado y Redis se reserva para caché opcional y no es la cola de tareas. Bun y Cargo son herramientas de desarrollo; Compose ejecuta servicios. `infra`, `scripts`, `skills` y `config` contienen entorno y capacidades integradas.

Código y datos de usuario están separados. No se publican discos, cuentas, chats, adjuntos ni claves. Las actualizaciones requieren artefactos versionados, copias de seguridad, migraciones y reversión; no un `git pull` directo sobre los datos del usuario.

## Futuro, responsabilidad y licencia

Prioridades: paquetes completos, recuperación de instalación, actualizaciones firmadas, estabilidad del Agente, audio/entrada, validación de modelos y Skills especializados. Después, voz y telefonía experimental; llamadas reales/eSIM aún no están implementadas. La mascota de escritorio salió del núcleo y queda como idea de extensión.

Interfaz y configuración inicial admiten diez idiomas. El selector a la izquierda de Skills cambia la interfaz; no traduce automáticamente el sistema invitado ni las salidas originales. El árabe usa escritura de derecha a izquierda.

El usuario responde de permisos, cuentas, acciones externas y condiciones de las aplicaciones. No usar para engaño, fraude ni acciones sin consentimiento. Código original Apache-2.0; los terceros conservan sus licencias. Esta declaración no elimina la responsabilidad legal.

[Estado](release-status.md) · [Arquitectura](architecture.en.md) · [Compatibilidad](compatibility.md) · [Terceros](../../THIRD_PARTY_NOTICES.md) · [Aviso](../../DISCLAIMER.md)
