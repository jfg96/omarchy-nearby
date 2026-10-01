# Estudio de arquitectura de Nearby y LocalSend

Fecha: 2026-10-01. Estudio en español, sin implementar ninguna alternativa.

## 1. Resumen ejecutivo

**Recomendación formal: evidencia insuficiente para aprobar la migración; realizar primero un spike específico.** Corregir el bloqueo de tamaño en un cambio independiente es la actuación inmediata aconsejada. La migración posterior queda condicionada a resolver las limitaciones descritas aquí; no se da por aprobada ni por inevitable.

El núcleo oficial es una biblioteca Rust real, sin dependencia de Flutter, y permite recibir decisiones de aceptación, seleccionar el destino del archivo, enviar datos, descubrir dispositivos y reutilizar certificados. Nearby puede conservar QML y su contrato JSON. Sin embargo, **el código oficial examinado no permite conservar toda la conducta actual mediante un adaptador fino que use exclusivamente su API pública**:

- El PIN del receptor es configuración inicial inmutable; falta un setter público en caliente. Reiniciar el servidor interrumpe conexiones y sesiones.
- El cliente oficial interpola los parámetros de consulta sin codificarlos. Nearby ya corrigió este problema para PIN salientes generales.
- El servidor oficial recoge JSON completo sin un límite explícito. Validarlo en `PrepareUpload` llega demasiado tarde para limitar esa asignación de memoria.
- El registro interno del descubrimiento oficial no tiene límite global ni caducidad; limitar solo el registro externo de Nearby no limita el interno.
- En Linux no existe destino de recepción por descriptor. `Path` crea o trunca el archivo y `Stream` devuelve a Nearby responsabilidades de escritura y validación.
- Cancelar una sesión oficial no detiene las escrituras ya iniciadas. Publicar un archivo validado y emitir el resultado correcto siguen siendo responsabilidades de integración.

La alternativa A tiene defectos concretos y coste de mantenimiento, pero también conserva endurecimiento útil y admite un parche pequeño para el bloqueo inmediato. La alternativa B reduce implementación propia del protocolo, pero no elimina automáticamente la superficie de revisión ni garantiza mejores límites de recursos.

## 2. Alcance, fuentes y nivel de evidencia

Nearby local: `/home/javi/Projects/omarchy-nearby`, commit `5b7c5b6d8f8726c4ee7e9d888757cea1201d5470`, helper 1.2.2. El árbol estaba limpio antes de crear este documento. No se actualizó el checkout ni se asumió que coincidía con un HEAD remoto posterior.

LocalSend oficial: clon de lectura en `/tmp/nearby-localsend-study-official`, commit `c5bbe3630bb50e0de8253502b41523c4a58825bb`, obtenido de `main` durante este estudio. Todas las afirmaciones sobre el núcleo se refieren a esa revisión, no a toda versión de LocalSend publicada.

Se leyeron las instrucciones del repositorio, README, USAGE, CONTRIBUTING, ARCHITECTURE, SECURITY, ROBUSTNESS, VENDORED_LOCALSEND_RS y CI. Se inspeccionaron el helper, settings, identidad, almacenamiento privado, servidor/cliente y descubrimiento del vendor, tipos, validación, guardado, contrato QML y pruebas pertinentes. Se contrastaron los módulos oficiales de servidor v2, guardado, PIN, cliente, TLS, descubrimiento, multicast, modelos y consumidores del CLI.

Fuentes oficiales fijadas al commit:

- [Manifiesto del núcleo](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/Cargo.toml).
- [API y configuración del servidor](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/server/mod.rs).
- [Eventos y sesiones v2](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/server/v2.rs).
- [Destinos y escritura](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/server/common/save.rs).
- [PIN](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/server/common/pin.rs), [JSON](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/server/common/collect_to_json.rs).
- [Cliente v2](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/client/v2.rs), [construcción de URL](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/http/client/url.rs).
- [Descubrimiento](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/discovery/mod.rs), [registro interno](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/packages/core/src/discovery/store.rs).
- [Licencia](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/LICENSE), [toolchain](https://github.com/localsend/localsend/blob/c5bbe3630bb50e0de8253502b41523c4a58825bb/rust-toolchain.toml).

Clasificación de hallazgos: **B** = defecto alcanzable identificado en el código; **R** = preocupación de robustez; **H** = endurecimiento opcional; **P** = conducta o limitación del protocolo. Los defectos descritos por trazado estático no se presentan como exploits ejecutados.

No se construyó una integración, no se ejecutaron transferencias reales ni se midieron tiempos de compilación o tamaño de un helper migrado. El spike de la sección 19 es un plan, no una prueba realizada. Este análisis tampoco es una auditoría exhaustiva de módulos opcionales como CLI, TUI, WebRTC o compartir por navegador.

## 3. Arquitectura actual y propiedad

```text
Panel.qml por monitor
        ↓
Service.qml compartido + Model.js
        ↓ JSON por stdin/stdout
launcher → helper Nearby
        ├ settings.rs / secure_state.rs / identity.rs
        ├ main.rs: política, orquestación, descubrimiento, estados y eventos
        ↓ API Rust
localsend-rs vendorizado
        ├ servidor HTTP: decisiones, sesiones, tokens, PIN, recepción
        ├ cliente HTTP: preparación, carga, cancelación y confianza TLS
        ├ multicast y sondeos HTTP
        └ tipos, hashing, rutas y publicación
        ↓
peer LocalSend
```

| Capa | Propiedad real |
| --- | --- |
| Panel.qml | Vista, foco, selección y popup por monitor |
| Service.qml / Model.js | Ciclo de vida, estado compartido, notificaciones, prompts PIN, correlación y comandos IPC |
| main.rs | Contrato JSON, aprobación, envío de archivos/texto, control de descubrimiento, TTL, límites de peers, directorio XDG y limpieza antigua |
| settings.rs | PIN persistente, perfil permitido, carga antes de escuchar y actualizaciones |
| secure_state.rs | Recorrido por descriptores, propiedad, no symlinks, límites, staging privado y sustitución durable |
| identity.rs | Validación y persistencia del certificado y clave; adaptación al tipo del vendor |
| Vendor servidor | HTTP, PIN de red, reserva de sesión, tokens, streaming, validación de integridad y finalización |
| Vendor cliente | HTTP LocalSend, cuerpos de upload, PIN de consulta, verificación TLS y respuestas |
| Vendor discovery | Sockets multicast, registro, sondeos HTTP/HTTPS e información de interfaces |
| Distribución Nearby | Helper precompilado, SHA256/tamaño, caché XDG, attestation y versiones |

El estado privado seguro no implica garantías equivalentes sobre Downloads. La documentación de seguridad ya distingue ambos límites.

## 4. Propiedad y mantenimiento del vendor

`VENDORED_LOCALSEND_RS.md` indica que el SHA base del import original es desconocido. Esto impide un rebase mecánico fiable contra CrossCopy. El traslado `0ac2d7d` preservó contenido; las modificaciones anteriores al traslado o incluidas en el import no quedan cuantificadas por comparar desde él.

La medición actual `git diff --stat 0ac2d7d HEAD -- backend/vendor/localsend-rs` da **622 inserciones y 80 borrados en 10 archivos**, con nueve commits de contenido visibles. El documento de mantenimiento conserva una medición anterior de 621/79 y ocho commits; este estudio usa la medición actual. Ninguna de las dos cifras constituye una comparación completa con upstream.

Áreas modificadas: registro HTTP preferente, netmasks reales, revalidación del puerto/protocolo cacheados, uploads desde memoria, PIN compatible con LocalSend, cambio de PIN en vivo, codificación URL y pruebas. El endurecimiento presente en handlers/session/path_safety también es responsabilidad de Nearby como distribuidor aunque no aparezca en ese diff posterior al traslado.

Conteo de archivos Rust: 50 archivos / 9.079 líneas bajo `src`, incluyendo tests y módulos CLI/TUI opcionales. Sin CLI/TUI/main quedan 32 archivos / 5.584 líneas; no son 5.584 líneas todas sensibles a seguridad. Tests externos: 12 archivos, 1.840 líneas y 37 atributos de prueba; `src` tiene 83 atributos, incluidos módulos opcionales. Helper: 39 pruebas en main, 11 settings, 9 identidad y 12 secure_state. Son conteos estáticos, no resultados de ejecución.

La mayoría de los 37 tests externos ejercita comportamiento que hoy se distribuye como implementación propia: preparación, aceptación/subconjuntos, sesión, recepción, integridad, PIN, TLS, texto y sharing. Migrar permite apoyarse en tests upstream, pero deben conservarse regresiones de integración para PIN, certificados, publicación y JSON. Eliminar todas esas pruebas sería perder evidencia útil.

La carga futura consiste en seguir interoperabilidad y seguridad de HTTP, TLS, tokens, sesiones, streaming y descubrimiento. La política congelada evita churn innecesario, pero obliga a revisar y portar cambios relevantes manualmente. Para un plugin pequeño es una carga considerable, aunque no prueba por sí sola que una migración resulte más barata.

## 5. Bloqueo de upload sobredimensionado

Confirmado en `src/server/handlers.rs::handle_upload` y `src/server/state.rs::write_body_to_file_with_progress`: el tamaño aceptado existe en `declared_size`, pero no se pasa al escritor. Cada chunk incrementa el contador y se escribe entero. La comparación ocurre tras consumir el cuerpo completo.

**B: un emisor autorizado puede declarar un archivo pequeño y transmitir indefinidamente más datos, consumiendo disco hasta EOF, error o agotamiento.** No necesita eludir el PIN: el problema aparece después de la aprobación. Un `Content-Length` no sustituye la contabilización de los chunks efectivamente recibidos.

Parche mínimo correcto, todavía no implementado:

1. Pasar `declared_size` al escritor como `expected_size`.
2. Convertir de forma segura la longitud del chunk y usar `checked_add` antes de escribir.
3. Si el siguiente total desborda o supera el tamaño, devolver error inmediatamente sin escribir ese chunk. No hace falta escribir su prefijo permitido.
4. Incrementar el progreso solo por chunks cuya escritura terminó correctamente. Mantener el límite del agregado y rollback de bytes previamente informados.
5. Conservar la comprobación final de igualdad para cuerpos cortos y la validación SHA256.
6. Garantizar cierre del archivo y eliminación del `.part` en error; mantener `SessionFailed`, supresión de `FileReceived`/`SessionCompleted` y el status HTTP previsto.
7. Distinguir, si procede, el error de tamaño del de conexión para que el mensaje al usuario no sea engañoso.

**Semántica actual importante:** aunque un comentario dice que la sesión se deja abierta para reintentar, el código llama a `fail_receive_session` y la elimina. El parche debe preservar la conducta ejecutada o cambiarla explícitamente en otro trabajo; no copiar el comentario como requisito.

Pruebas necesarias: cuerpo exacto, cero bytes autorizado, cuerpo corto, primer chunk demasiado grande, exceso en un chunk posterior, suma desbordada mediante función de comprobación, fuente que falla si se consume después del chunk rechazado, cleanup y progreso sin bytes excedentes. La prueba del exceso debe fallar contra el escritor actual antes del arreglo.

El cambio es pequeño y no exige tocar settings, identidad ni QML. La ruta cercana sí contiene otras suposiciones problemáticas; no es correcto afirmar que este parche sanea todo el receptor.

## 6. Alternativa A: auditoría concreta de límites actuales

| Área y clasificación | Evidencia y consecuencia |
| --- | --- |
| Tamaño de upload — B | Escritura ilimitada antes de comparar; bloqueo actual confirmado |
| Upload simultáneo/repetido — B | No hay estado por archivo InProgress/Finished que impida reutilizar el token. Dos POST válidos comparten el mismo `.part`; ambos ejecutan `remove_file`, pueden separar inode y pathname o fallar mutuamente. En una sesión de varios archivos, un archivo ya completado sigue teniendo token válido |
| Cancelación — B/R | `handle_cancel` borra la sesión, pero el escritor no recibe cancelación; sigue consumiendo disco hasta acabar. La revalidación final evita publicar contra otra sesión, no detiene el coste de I/O |
| Sustitución de sesión pendiente — B | La respuesta de aprobación/declinación no comprueba que la reserva siga siendo suya. Si se cancela la reserva y empieza otra solicitud mientras la decisión vieja espera, la decisión vieja puede borrar o reemplazar la nueva sesión |
| Sesión al finalizar upload — protección existente | Se vuelve a comprobar el ID antes de publicar/contabilizar; es una defensa útil que debe conservarse |
| Travesía/nombres anidados — protección existente | `safe_join` rechaza absoluto, `..`, backslash, colon, NUL y más de un componente normal. Hoy no hay transferencia de carpetas por rutas anidadas |
| Nombre temporal — R | Se deriva de sessionId/fileId; FileId admite strings y no se valida como componente local. Puede provocar errores de ruta; no se demuestra aquí un escape remoto general. Conviene un nombre aleatorio independiente de identificadores remotos |
| Creación temporal — protección + R | `create_new` evita seguir un objeto final ya existente. La eliminación previa del pathname y el nombre compartido entre intentos debilitan esa protección frente a carreras |
| Colisiones/publicación — protección + R | `hard_link` publica sin sobrescribir, con sufijos y retries en colisión. Si el unlink posterior falla, puede quedar publicado el final aunque se informe fallo. No hay fsync de archivo/directorio de descarga |
| Symlinks — protección + supuesto local | Los nombres remotos planos evitan padres remotos symlinked, y hard_link no sobrescribe destinos existentes. Downloads se canonicaliza al inicio; no queda fijado por descriptor frente a sustitución posterior del directorio. No equiparar esto a secure_state |
| Checksum — protección + P | Se verifica SHA256 si fue declarado, después de escribir el temporal y antes de publicar. No autenticación del contenido frente al propio emisor malicioso; ausencia de checksum no es corrupción demostrada |
| Tokens — protección + P | Aleatorios por archivo, se comparan con sesión/archivo. Upload y cancel no vinculan la solicitud a la IP del emisor; upload exige token y cancel conoce solo sessionId |
| Subconjunto aceptado — protección | La sesión definitiva contiene únicamente IDs ofrecidos y aceptados. Conservar la correlación y filtrar IDs desconocidos |
| Conteo/tamaño total — R/B en agregado | No hay máximo de archivos o total ofrecido. Algunos totales usan saturación y otros `sum::<u64>()`; tamaños maliciosos pueden desbordar el total comunicado por el helper. El límite JSON no restringe el valor numérico ni evita ese error |
| Texto entrante — R | El shortcut usa tamaño declarado <1 MiB y preview no vacío, sin comparar longitud real de preview ni MIME. El límite de salida aplica caracteres tras parsear; no equivale a limitar bytes entrantes a 1 MiB |
| JSON entrante — protección parcial | Los extractores Json de Axum tienen límite por defecto de 2 MiB; `Body` directo del upload no hereda ese límite. No declarar todos los cuerpos ilimitados ni todos limitados |
| Colas — R | Eventos del servidor y peer_tx son unbounded. Progreso se limita aproximadamente a 75 ms, registro/discovery no. stdout bloqueado puede favorecer acumulación |
| Mapas — protección + R | Registry Nearby: 256 peers y TTL 90 s; PIN: LRU 200; pendientes se retiran en decisión/expiración. El límite externo no limita tareas de respuesta multicast ni colas anteriores |
| Conexiones/churn — R | Sin límite explícito de conexiones del servidor; multicast crea tareas por anuncio sin semaphore y puede sufrir ráfagas. No se probó agotamiento real |
| Timeouts — protección + R | Aprobación 60 s, barrido de sesión ociosa 300 s cada 60 s. active_uploads impide expirar una carga lenta; también permite retener la sesión con un upload que no progresa. Falta timeout de inactividad del cuerpo |
| Directorio elegido — supuesto de producto | Respeta XDG Downloads. Disco lleno, permisos, volumen sin hardlinks y cambios locales necesitan errores coherentes; no son por sí solos ataques remotos |
| Descubrimiento/TLS — P + R | LAN discovery no es pairing. Multicast acepta metadatos y el peer se registra antes de confirmar HTTP; HTTPS saliente usa fingerprint. Registro entrante no demuestra correspondencia con certificado de cliente |
| Sharing por navegador — alcance reducido | Rutas presentes en router, web_share desactivado por Nearby. No extrapolar riesgos de una sesión compartida que el helper no habilita |

Prioridad razonable: tamaño primero; después exclusión/reutilización de upload y cancelación; luego reserva de sesión, sumas/límites y recursos. Fsync de Downloads, cuotas opcionales o política más estricta sobre directorios son endurecimiento separado; no deben convertirse en un refactor interminable.

Referencia del límite Axum: [DefaultBodyLimit en la versión 0.8.9 del lockfile](https://github.com/tokio-rs/axum/blob/axum-v0.8.9/axum-core/src/extract/default_body_limit.rs).

## 7. Alternativa B: reutilización real de la API oficial

El paquete se llama `localsend`, versión 0.1.0, edition 2021. El workspace usa resolver 3 y el toolchain oficial fija Rust 1.97.1, igual que CI de Nearby. No declara `rust-version`: no se establece aquí el MSRV mínimo. La biblioteca expone módulos y se usa tanto desde el CLI como desde el puente Flutter; el CLI enlaza `features = ["full"]`. Eso demuestra reutilización técnica interna, no un compromiso de compatibilidad para consumidores externos.

La API pública es suficiente para un prototipo: `start_with_port`, `ServerConfigV2`, `TlsConfig`, canales de eventos, decisiones, `FileUploadTarget`, `LsHttpClientV2`, `DiscoveryHandle` y `FileContent`. La versión 0.1.0 y falta de una política de estabilidad constatada obligan a tratar cambios upstream como potencialmente incompatibles.

`ServerConfigV2` contiene PIN, verify_checksums y un Sender bounded. `ServerHandle` permite consultar puerto/direcciones, esperar parada y cancelar una sesión activa, pero no actualizar PIN, configurar límites JSON, podar el store de discovery o detener una carga específica. Los estados relevantes son privados; que `AppState` sea público no los hace accesibles.

Ventajas concretas: conexión global/per-IP limitada a 64/8, colas con contrapresión, registro TLS con comprobación del fingerprint del certificado, upload vinculado a IP, estados por archivo, guards de limpieza de estado, verificación checksum y soporte IPv6. No equivalen a una garantía de seguridad global: JSON sin límite, store sin poda y tareas multicast por anuncio son límites aún abiertos.

Dependencias: HTTP/TLS/Tokio ya existen en Nearby. El core añade o cambia crypto, RSA, ed25519, x509-parser y otras versiones. `full` agrega WebRTC, WebSocket y compresión que Nearby no necesita. Conviene ensayar `discovery` —que activa http+multicast— y comparar con `full`; las instrucciones/CI upstream solo garantizan su ruta full y señalan problemas con builds sin features. **No se asegura aquí que la combinación mínima compile.**

No exige Flutter ni ejecutar LocalSend instalado, ni agrega por diseño otra dependencia runtime a Omarchy. El aumento de bytes, RSS, crates y tiempo de build queda sin medir. Debe compararse con el helper publicado de 9.040.128 bytes y el límite de descarga de 32 MiB del launcher.

## 8. Diff de comportamiento PIN

| Conducta | Nearby hoy | Núcleo oficial examinado | Adaptación necesaria |
| --- | --- | --- | --- |
| Persistencia | settings.json privado | No la proporciona el core | Reutilizar settings/secure_state |
| Carga antes de escuchar | Sí | PIN en ServerConfigV2 | Mantener orden de arranque |
| Estado inválido | startup_failed y sin listener | Política externa | Mantener fail-closed |
| Cambio/disable en caliente | set_pin, sin reiniciar | No API pública | Cambio upstream o fork; bloqueo de paridad |
| Falta PIN, sin bloqueo | 401, no suma intento | Igual | Mapeo |
| PIN incorrecto | 401 y suma | Igual | Mapeo |
| Tres errores | Tercer error 401; solicitudes posteriores 429 | Igual | Conservar tests exactos |
| PIN correcto después de dos errores | Limpia contador | Igual | Sin política adicional |
| PIN correcto después de lockout | Sigue 429 | Igual | Sin política adicional |
| Cuenta por IP | LRU 200 IPs | LRU 200 IPs | IPv6/scope en integración |
| Tiempo de lockout | Sin cooldown, hasta reset/reinicio o eviction | Igual en check_pin | No prometer bloqueo persistente |
| Cambio PIN limpia fallos | Sí | No operación equivalente | Setter upstream con reset |
| Sesión ya autorizada tras cambio | Continúa con tokens | No se puede cambiar PIN en vivo | Reiniciar rompe la garantía |
| PIN entrante general | 1–64 ASCII alfanumérico y . _ ~ - | String sin ese perfil | Mantener validación Nearby |
| Espacios/Unicode/reservados entrantes | Nearby los rechaza por producto | Core no impone ese rechazo | No ampliar silenciosamente |
| PIN saliente general | Admite hasta 4.096 bytes | Recibe Option<&str> | Mantener límite local |
| Codificación saliente | query_pairs_mut correcto | TargetUrl concatena strings | Precodificar exactamente una vez |
| Comparación | Igualdad sin salida temprana en bytes de igual longitud | Igualdad String ordinaria | H; no requisito de paridad funcional |
| Prompts QML | 401 sin PIN → prompt; con PIN → incorrecto; 429 → error específico | ClientError con status | Mapear sin alterar QML |

La codificación saliente puede envolverse: serializar el valor con codificación de query antes de llamar al cliente oficial, dado que este actualmente no codifica. Probar espacio, Unicode, `+`, `&`, `#`, `%` y secuencias como `%26`, para impedir doble codificación. Preferible arreglo upstream y retirada del workaround cuando cambie la API.

El PIN entrante no se puede envolver después de `PrepareUpload`: el core ya pasó por su gate y el evento no entrega el PIN original. Deshabilitar el gate y validar en el evento tampoco funciona. Proxy HTTP/TLS o servidor propio delante del core reconstruirían una frontera de seguridad importante y debilitarían el objetivo. Reinicio o demora del cambio requieren una modificación visible del producto, no paridad automática.

## 9. Descubrimiento

| Conducta | Nearby actual | Oficial | Decisión de integración |
| --- | --- | --- | --- |
| Multicast pasivo | Mientras helper habilitado | start + escucha | Conservar lifecycle |
| Anuncio activo | Inicial inmediato y reintentos | Burst con delays 100, 500, 2.000 ms entre envíos | No afirmar misma temporización |
| Respuesta register | Vendor y evento helper | Register del server debe alimentarse a add_device | Cablear ambas direcciones |
| Cache-first | Grace 1 s; revalidación reciente; cache hit evita scan | discover_known_http_channels + discover_staged | Mantener política de la ronda |
| TTL | 90 s | Store sin TTL | Adaptador externo no poda store interno |
| Límite peers | 256 externo | Store Vec sin límite de peers/canales | API upstream o usar multicast de bajo nivel |
| HTTP fallback | Register primero, luego info; alterna HTTP/HTTPS | Discovery usa register y esquema especificado | Usar cliente info y alternancia en adaptador |
| Subredes | Máscara real, hasta /22; grandes → /24 local | scan_subnet siempre /24, 50 concurrentes por scan | Conservar rangos y límite global de Nearby |
| Cache puerto/protocolo | Conservados | HttpChannel los conserva | Reutilizable |
| IPv4/IPv6 | Multicast/scan actual IPv4; cliente no construye bien URLs IPv6 sin brackets | Multicast IPv6 y hosts con scope | Mejora potencial, probar contrato ip |
| Identidad/dedup | Fingerprint | Fingerprint con múltiples canales | Traducir a device JSON, evitar oscilación de dirección |
| Confirmación TLS | PIN del fingerprint en conexiones conocidas; HTTP fallback acepta certs para descubrir | Toma fingerprint del certificado HTTPS | Preservar binding al certificado |
| Caso iOS stale-open | Fallback específico histórico, versiones no registradas | Escalado disponible pero no idéntico | Ensayo real Android/iOS |
| Search for new devices | force_full bypass cache-hit | Operaciones de scan separadas | Mantener comando |

Una vía sin fork para límites de discovery es usar `multicast` de bajo nivel más el cliente oficial y conservar el registro bounded de Nearby. Esto evita el store oficial, pero retiene más orquestación. Usar el discovery de alto nivel y mantener otro registro de 256 no resuelve el crecimiento de su Vec interno. Reinicios periódicos del discovery implican pérdida de estado y no son una solución fina acreditada.

Multicast/register son protocolo; TTL, prioridades de cache, límite global de scans, timing y force_full son política Nearby. No borrarlos solo porque el core tiene un método llamado discover_staged.

## 10. Envío, separado de recepción

Nearby fabrica metadatos, prepara upload, interpreta 401/429, itera el subconjunto de tokens aceptados, carga fichero o bytes, limita eventos de progreso y cancela con correlación transferId. No genera SHA256 saliente actualmente; los metadatos llevan sha256=None. Tampoco implementa retry automático de checksum. No presentar esas capacidades como existentes a conservar.

El cliente oficial sustituye endpoints, serialización DTO, TLS del cliente, upload y cancel HTTP. `LsHttpClientV2::try_new` acepta PEM de identidad y fingerprint esperado; este último se verifica durante handshake. `None` solo es apropiado para discovery, nunca para enviar archivos. Normalizar fingerprint a hexadecimal mayúscula para la API oficial sin cambiar el certificado persistente.

`upload` recibe `reqwest::Body` y CancellationToken; no lleva callback de progreso ni construye automáticamente metadatos/checksum de archivos locales. Nearby debe construir un stream instrumentado, usar FileContent::Path o memoria, contabilizar bytes, decidir política de checksums/retries y mapear ClientError. El core reexporta reqwest, lo que permite usar el mismo tipo Body.

Hay apoyo a subconjuntos mediante tokens retornados y respuesta 204 sin upload. Texto es política del consumidor: Nearby puede conservar preview y la carga en memoria si el peer devuelve tokens. CancelReceived requiere validar tanto IP como sessionId del receptor remoto antes de cancelar un outgoing.

El core permite al receptor reintentar checksum hasta tres uploads; el cliente no orquesta automáticamente esos intentos. Agregarlos sería conducta nueva que exige rebobinar Body y hacer rollback de progreso. Mantener sin retries automáticos es una primera migración de menor alcance.

Se podría eliminar `LocalSendClient`, el adaptador trust_policy propio del vendor y la implementación de endpoints. Seguirían selección de fuentes, metadatos, texto, progreso, estados, correlación, límites y política de reintento.

## 11. Recepción y guardado

| Garantía | Vendor actual | Core oficial |
| --- | --- | --- |
| Tamaño durante escritura | No, bloqueo actual | Path/Fd rechazan el chunk que supera esperado; suma sin checked_add |
| Tamaño exacto final | Sí | Sí en Path/Fd; Stream lo exige al consumidor |
| Checksum | Hash de temporal antes de publicar | Hash durante forwarding; configurable verify_checksums |
| Destino Path | Temporal create_new y hardlink final | File::create, crea/trunca y sigue resolución de path |
| Destino por descriptor | No API dedicada; escritor propio | Solo Android, no Linux |
| Destino Stream | No usado por Nearby | Bounded chunks; app decide resultado |
| Finalización atómica/collision | hardlink con sufijo, sin overwrite | No la proporciona el core |
| Cleanup parcial | Helper/handler + limpieza >24 h | App debe gestionar path y errores |
| Timestamps | Nearby no los aplica | Path/Fd los aplica best-effort |
| Cancelación de sesión | No corta escritura, no publica si sesión cambió | Cargas iniciadas continúan; app debe impedir publicación |
| Reutilización token | Sin estado de upload por archivo | Solo Pending; InProgress/Finished impiden upload duplicado |
| Reintento checksum | Fallo elimina sesión | Hasta 3 intentos con token igual |
| Fin de sesión | Completed/Failed diferenciados | Finished incluye archivos finalizados con éxito o fallo |

**No pasar el nombre remoto como path final al core.** Path carece de collision naming y staging; podría truncar un archivo existente. La app selecciona la ruta, por lo que esas garantías son de Nearby.

Dos diseños que merecen comparar:

- **Path a staging controlado:** temporal en un directorio privado y aleatorio del mismo filesystem, separado del nombre remoto; verify_checksums=true; esperar result_tx y validar que la sesión siga autorizada antes de hardlink y cleanup. Mantiene tamaño/checksum en el core. La reapertura por pathname sigue requiriendo justificar seguridad de todos los padres y sus cambios; no equivale al descriptor fijo de secure_state.
- **Stream con archivo abierto por Nearby:** permite create_new/no-follow y cancelación del escritor bajo control de Nearby. Sin embargo debe contar bytes antes de escribir y hacer checksum local antes de publicar. El resultado de checksum calculado posteriormente por el core no se comunica a la app en Stream. Devolver Ok y publicar solo por ese Ok es incorrecto. Esta opción retiene una parte sensible de la implementación de recepción.

Path informa éxito antes de que Nearby termine su publicación. Hay que decidir qué hacer si hardlink falla después de que el core considere recibida la carga: el HTTP podría indicar éxito aunque Nearby no consiga el nombre final. Stream permite incluir publicación en la decisión del consumidor, pagando más implementación local. Esa diferencia es un criterio de aceptación del spike, no un detalle menor.

Con cualquiera de las dos opciones, `SessionEnd::Finished` no debe convertirse directamente en incoming_done. El adaptador tiene que esperar los resultados por archivo y todas sus publicaciones; un hash mismatch puede admitir retry, y un Finished puede contener fallos. Mantener un estado terminal único evita que eventos tardíos contradigan cancelación o error.

## 12. Identidad y TLS

**Conservar identidad, formato persistente y secure_state de Nearby.** La API `TlsConfig { cert, private_key }`, DeviceIdentity de discovery y constructor del cliente admiten PEM. No existe una necesidad técnica de rotar la identidad ni adoptar almacenamiento del CLI.

`identity.rs` depende hoy del tipo TlsCertificate y funciones del vendor; será necesario desacoplar ese tipo y la generación/hash, preservando la validación rustls, límites, propietario, permisos y bytes ya guardados. El fingerprint oficial es mayúscula; Nearby usa hash hexadecimal de su biblioteca. Normalizar representación, no cambiar el material criptográfico.

El generador oficial crea RSA-2048; Nearby genera con rcgen una identidad que debe probarse contra los verificadores x509 oficiales antes de prometer compatibilidad. Ambas APIs admiten PEM, pero esa observación no sustituye el ensayo de firma/algoritmo y mTLS.

El servidor oficial exige certificado cliente cuando WebShare está Disabled y valida firma/validez; comprueba el fingerprint declarado en register. Nearby hoy configura from_pem con el comportamiento estándar de axum-server, sin su propio verificador mTLS. Esto es un cambio real de interoperabilidad para clientes antiguos o terceros y debe probarse. No habilitar sharing por navegador para eludir ese cambio de confianza.

La identidad no debe regenerarse en cada arranque o transferencia. Rotación solo por una decisión independiente y explícita, no como efecto colateral del engine swap.

## 13. Mapa completo de comandos y eventos QML

Los comandos usan snake_case por serde; mantener payloads y correlación requestId/transferId/sessionId.

| Comando actual | Implementación propuesta |
| --- | --- |
| discovery_start {force_full} | Orquestación multicast/sondeos conservando escalado y flags |
| discovery_stop | Cancelar ronda activa; escucha pasiva sigue |
| accept {request_id} | Lookup de decision_tx, Accept(IDs) y eventos locales |
| decline {request_id} | Decision::Decline, retirar pendiente |
| send_files {transfer_id,device,paths,pin} | Cliente oficial + fuentes + stream/progreso |
| send_text {transfer_id,device,text,pin} | DTO preview + memoria cuando haya upload |
| cancel_outgoing {transfer_id} | CancellationToken y cancel HTTP del sessionId remoto |
| set_incoming_pin {pin} | Persistencia existente; bloqueado en API oficial actual para aplicar en caliente |
| disable_incoming_pin | Misma limitación |
| shutdown | Stops, wait_stopped, cleanup y fin del helper |

| Evento actual | Fuente/adaptación |
| --- | --- |
| ready | Emitir tras iniciar server y discovery; helperVersion, alias, directory, port, fingerprint |
| startup_failed | Fallo de settings/port y clasificación actual; no atribuir ready a un listener TLS que falla después |
| incoming_pin_state | Settings + aplicación real del cambio, nunca mostrar PIN guardado |
| incoming_pin_update_failed | Error de persistencia/aplicación; conservar valor anterior y fail-closed |
| peer_snapshot | Registro bounded/TTL de Nearby |
| device | DiscoveryEvent/Register → shape DeviceInfo existente |
| discovery_started / discovery_stopped | Inicio y fin de ronda activa |
| incoming_request | PrepareUpload → requestId propio o sessionId, sender/files/total |
| incoming_accepted / incoming_declined | Confirmación local al enviar decisión válida |
| incoming_expired | Timer local de 60 s y PrepareUploadAborted; retirar canal una sola vez |
| incoming_text | Shortcut preview aprobado → Accept(empty set), texto limitado por bytes |
| incoming_progress | Progress de Path o escritor Stream, agregado solo aceptados y por sesión |
| file_received | Tras checksum local/core y publicación final exitosa; path/nombre reales |
| incoming_done | Finished + todos los resultados y publicaciones exitosos |
| incoming_cancelled | SessionEnd Cancelled y cleanup; impedir terminal de éxito posterior |
| incoming_failed | Resultado por archivo, publicación, expiración o fallo de sesión |
| outgoing_preparing | Metadatos seleccionados y transferId |
| outgoing_progress | Stream instrumentado y throttling existente |
| outgoing_done | 204 o todos los uploads elegidos confirmados |
| outgoing_cancelled | Cancel local o CancelReceived autenticado por destino/sesión |
| outgoing_pin_required / outgoing_invalid_pin | 401 diferenciado por si se proporcionó PIN |
| outgoing_failed | ClientError, 429 y errores locales; mensajes existentes |
| error | Comandos inválidos o fallo general del helper |

ListenerFailed debe provocar un fallo visible y lifecycle coherente, aprovechando la recuperación de Service.qml; no existe un evento frontend con ese nombre que deba introducirse por obligación. Se puede reportar error y terminar el helper. Los eventos del reparador `step/done/failed` pertenecen a otro proceso y permanecen sin cambio.

El adaptador no debe bloquear su bucle de eventos esperando una decisión o una recepción completa: eso impediría procesar cancelaciones y otros archivos. Guardar responders y lanzar tareas de I/O con límites.

## 14. Dependencia, reproducibilidad y licencia

| Opción | Evaluación |
| --- | --- |
| B1: Git con rev exacta | Preferida para spike y posible integración. SHA explícito + Cargo.lock del helper + --locked en CI. Updates deliberados con revisión de diff y regresiones |
| B2: vendor oficial | Facilita build sin red tras vendorización completa y auditoría local. Sin cambios locales, permite diff exacto contra SHA; si se parchea vuelve a existir un fork mantenido. Vendorización no quita responsabilidad de revisar |
| B3: crates.io | No disponible como equivalente oficial verificado. API crates.io devuelve localsend 0.2.2 de wylited/localsend, otro proyecto; no sustituir por nombre |
| B4: CLI subprocess | Rechazar. CLI de terminal/TUI, sin contrato JSON machine-readable equivalente; lifecycle, decisiones, PIN y progreso requerirían parsing frágil y otro binario |

La consulta a [API de crates.io](https://crates.io/api/v1/crates/localsend) identificó el paquete homónimo de otro repositorio. La publicación oficial no puede inferirse del nombre en Cargo.toml.

Para B1, Cargo descubre el package en el repositorio Git; fijar `rev`, no branch/tag flotante. Cargo.lock fija dependencias transitivas del helper, pero no garantiza disponibilidad futura de los servidores de descarga. Builds offline necesitan caché previamente completa o cargo vendor, incluida la fuente Git. No usar el lockfile del workspace oficial como sustituto del del helper.

El artefacto publicado debe seguir el modelo Nearby: CI del commit exacto, hash/tamaño, attestation y pin del helper. La attestation de Nearby no convierte el código upstream en una dependencia auditada ni acredita todas sus fuentes por separado. Registrar SHA oficial/features en SBOM/notices y revisar la licencia/dependencias al actualizar. El core oficial está bajo LICENSE Apache-2.0 del repositorio, mientras Nearby/vendor actual declaran MIT; su Cargo.toml no declara license. Conservar licencia, atribución y avisos aplicables en el paquete/distribución; esto requiere actualizar THIRD_PARTY_NOTICES, no reescribir el producto.

## 15. Coste y fases de una migración eventual

Los nombres de módulos nuevos son una propuesta, no archivos creados. Complejidad y esfuerzo son estimaciones de ingeniería basadas en las responsabilidades observadas, no mediciones.

| Fase | Archivos probables | Complejidad y riesgo | Reutilizar / eliminar |
| --- | --- | --- | --- |
| Dependencia/PoC | Cargo.toml, Cargo.lock, harness aislado | Media: features, API, toolchain, bytes | Toolchain CI / futura dependencia path antigua |
| Discovery | main.rs, posible discovery.rs | Alta: TTL/store, /22, fallback, timing | Registry/rangos/política / sondeos y sockets del vendor |
| Prepare/decisiones | main.rs, posible receive.rs | Media: deadlines y correlación | JSON/pendientes / reserva manual del vendor |
| Destino/publicación | receive.rs, posible file_policy.rs | Alta: descriptor Linux, checksum, ack HTTP, cancel | Nombres planos/hardlink/cleanup / escritor según destino |
| PIN | settings.rs, main.rs, API upstream | Alta: setter inexistente | Persistencia/validación / gate vendor si hay equivalencia |
| Outgoing | main.rs, posible send.rs | Media: streams, subset, queries | Estados/metadatos/texto / endpoints/TLS vendor |
| Identidad | identity.rs | Media: tipo, hash/case, algoritmo | Secure state y PEM existente / tipo vendor |
| Cancel/progreso | main.rs, receive.rs, send.rs | Alta: tareas y terminales tardíos | Throttle/correlación / primitivas vendor |
| Contrato QML | main.rs, tests/service-state.test.js | Media: event ordering | QML/Model.js / nada inicialmente |
| Tests | backend tests y tests frontend | Alta: límites y peers reales | Regresiones de producto / tests internos duplicados tras paridad |
| Retirar vendor | vendor/, Cargo, notices | Baja después de paridad | Atribuciones necesarias / árbol antiguo |
| Docs/release | docs, workflows, build.sh, metadata cuando toque | Media: reproducibilidad/size | Modelo de distribución / documentación vieja |

Orden de magnitud: prototipo limitado **varios días de trabajo concentrado**; paridad **varias semanas**, incluyendo los puntos de API; estabilización **otra o varias semanas** con Android/iOS y carreras. La aceptación de cambios upstream puede añadir espera externa sin límite estimable. No se promete una fecha ni una migración completa en un fin de semana.

El arreglo de tamaño actual es de complejidad baja, con una firma, caller, comprobación de chunks y pruebas; no justifica esperar a la migración para eliminar el defecto.

## 16. Comparación directa

| Criterio | Parchear vendor actual | Núcleo oficial |
| --- | --- | --- |
| Esfuerzo inmediato | Bajo para tamaño; más para rutas cercanas | Medio/alto, no sustituye un hotfix pequeño |
| Riesgo de migración | Bajo por cambio acotado | Alto hasta resolver PIN/guardado/discovery |
| Mantenimiento largo plazo | Fork/protocolo propios | Menor si upstream cubre requisitos; fork oficial debilita ventaja |
| Compatibilidad de protocolo | Probada por tests locales, real-device pendiente | Canónico actual; versiones móviles concretas por probar |
| LOC propias sensibles | Miles del vendor + integración | Menos HTTP/TLS/sesiones; siguen política de archivos y streams |
| Superficie Marketplace | Código enviado y modificado directamente | Dependencia sigue revisable; integración más estrecha |
| Paridad PIN | Existente | Gate similar; runtime setter ausente |
| Paridad discovery | Política acumulada existente | Parcial; store/límites/scan difieren |
| Identidad TLS | Persistente y endurecida | Reutilizable; verificar algoritmo y case |
| QML | Sin cambios | Puede mantenerse con traducción y estados |
| Complejidad de build | Conocida y locked | Git/features/versions; full añade WebRTC |
| Estabilidad dependencia | Congelada; base upstream desconocida | SHA reproducible, API 0.1.0 sin promesa externa constatada |
| Actualizar upstream | Portado manual difícil | Pin updates más limpios si no hay parches |
| Urgencias | Se puede parchear directamente | Esperar upstream o override/fork temporal |
| Tests | Protocol suite propia + integración | Protocol suite upstream + regresiones Nearby |
| Limpieza estratégica | Menos limpia, pero coherente actualmente | Mejor si existe adaptación estrecha sin proxy/fork sostenido |

La comparación no da un ganador incondicional: el oficial tiene ventajas claras de protocolo y límites de conexiones, mientras su API actual no cubre requisitos importantes del producto.

## 17. Marketplace y responsabilidad

Migrar cambia quién mantiene las primitivas, no qué bytes distribuye Nearby. Un fallo upstream de JSON, TLS o sesión debe comunicarse/corregirse allí cuando proceda, pero Nearby sigue obligado a actualizar el pin o mitigar y a no distribuir indefinidamente una revisión vulnerable.

Siempre son de Nearby: políticas de aprobación, límites de producto, persistencia, creación/publicación/cleanup seleccionados, PIN UX, contrato QML, distribución, lifecycle y uso correcto del fingerprint. Un wrapper que olvide codificar PIN, entregue un path remoto, convierta Finished en éxito o use cliente sin pin puede introducir un defecto propio sobre un core correcto.

Marketplace puede revisar el comportamiento de dependencias. El objetivo legítimo es reducir código propio sensible y alinearse con el motor canónico, no impedir hallazgos de revisión.

## 18. ¿Puede ser una capa fina sobre el motor oficial?

**En gran parte sí; con paridad completa y sin cambiar/forkear el core actual, no queda demostrado y existen bloqueos concretos.**

```text
QML / Service.qml / Panel.qml existentes
                ↓ contrato JSON existente
Nearby adapter / policy
  settings, secure_state, identidad persistente
  aprobación/expiración, límites, correlación, progreso
  publicación/colisiones/cleanup, Omarchy y distribución
                ↓ API Rust
LocalSend oficial fijado a SHA
  HTTP, TLS, sesiones/tokens, cliente, checksum
  multicast; discovery alto nivel solo si sus límites encajan
```

Condiciones para recomendar este split: PIN runtime upstream; límite JSON anterior a parseo; destino de guardado que permita preservar confirmación/publicación segura; discovery interno bounded/podable o elección explícita del módulo bajo nivel. Sin ellas, se conserva demasiada infraestructura propia o se incurre en regresiones.

Mantener: frontend, contrato JSON, lifecycle Omarchy, settings, secure_state, identidad guardada, launcher/verificación/attestation, estados/progreso, TTL/cache/scan útiles y diferencias de política justificadas. No se propone una reescritura de UI, settings o distribución.

## 19. Siguiente paso concreto: spike mínimo, sin implementarlo aquí

Primero preparar por separado el parche del oversized body y sus tests. Para evaluar B, una rama aislada de estudio con harness o helper experimental, sin retirar vendor ni modificar release metadata, debe demostrar:

1. Build con revisión exacta y toolchain 1.97.1; probar full y features mínimas. Medir tiempo/ELF y revisar árbol de dependencias.
2. Arranque TLS en 53317 usando una identidad Nearby existente de prueba; parada y rebinding; error de puerto claro.
3. Discovery con Android e iOS oficiales, registrando versiones; round activa con popup y escucha pasiva sin popup.
4. PrepareUpload emitido como incoming_request; Accept/Decline, expiración de 60 s y desconexión del emisor.
5. Un archivo byte-for-byte con destino Path de staging o Stream; exacto, exceso, truncado, hash mismatch, colisión, fallo de publicación y cleanup.
6. PIN 401/429/LRU; PIN antes del arranque; cambio en plena carga. **Con la API actual se espera detectar ausencia de setter**, no fingir una prueba de paridad exitosa.
7. Emitir JSON actual y comprobarlo con el consumidor/test frontend, sin modificar QML; done solo tras publicación.
8. Cancelación mientras se escribe y sesión nueva; ningún file_received ni done de la cancelada; corte de consumo y ausencia de `.part` huérfanos.

Criterios de decisión: si PIN en vivo exige reinicio/proxy/fork permanente, B no cumple el objetivo sin renegociar producto o lograr una API upstream. Si Path no alinea HTTP success y publicación, evaluar Stream y cuantificar exactamente la responsabilidad retenida. Si discovery alto nivel no limita memoria, usar bajo nivel de forma explícita y medir el tamaño del adaptador.

Posibles propuestas upstream, todavía no enviadas: setter de PIN con reset LRU sin invalidar sesiones; JSON byte limit; límite/TTL/poda del store; destino por archivo/descriptor en Unix o confirmación de publicación; cancelación por upload. No se presupone su aceptación.

## 20. Preguntas pendientes y conclusión

- ¿Compila el conjunto mínimo de features y cuánto crece el helper respecto a 9 MB?
- ¿Aceptan los verificadores oficiales la identidad rcgen ya persistida de Nearby con mTLS y su fingerprint normalizado?
- ¿Qué camino de recepción preserva atomicidad, checksum, cancelación y coherencia de la respuesta HTTP sin reconstruir el motor?
- ¿Puede upstream incorporar PIN runtime y límite JSON de forma mantenible?
- ¿El descubrimiento bajo nivel conserva el caso iOS stale-open, scan /22, fallback y TTL sin un adaptador demasiado grande?
- ¿Qué cambios de API hay entre revisiones etiquetadas que realmente interesan a Nearby?
- ¿Debe introducirse política explícita de count/total/preview sin romper transferencias normales?

**Decisión actual: corregir el defecto inmediato; mantener temporalmente el vendor y validar la migración con el spike descrito.** Si se resuelven las carencias con API upstream y el adaptador conserva publicación/contrato, migrar en una release separada será una estrategia razonable. Si requiere otro fork amplio, proxy o regresiones de PIN, continuar endureciendo el vendor puede ser la opción más sensata.

Durante la fase de estudio no se modificó código de producción ni se implementó el spike o el parche. Este documento describe las revisiones indicadas en la sección 2 y distingue lo comprobado en fuente de lo que faltaba probar en ejecución; el arreglo posterior y sus validaciones se registran por separado en VENDORED_LOCALSEND_RS.md y ROBUSTNESS.md.
