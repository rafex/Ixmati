Inicializa SpecNative para este proyecto guiando al desarrollador a definir su contexto base. Llena los documentos core con contenido real del proyecto.

## Proceso

1. Llama `health_check()` via MCP para ver qué documentos están vacíos o incompletos.

2. Presenta al desarrollador un resumen breve (1-2 líneas):
   > "Voy a ayudarte a definir el contexto base de tu proyecto en SpecNative. Haré preguntas en orden y llenaré los documentos con tus respuestas. Empecemos."

3. Haz las siguientes preguntas **en orden**, espera respuesta antes de continuar:

   **Producto:**
   - ¿Qué problema principal resuelve este proyecto?
   - ¿Para quién lo construyes? ¿Cuál es su dolor principal?
   - ¿Cuál es el objetivo concreto y medible de éxito?
   - ¿Qué queda explícitamente fuera del alcance?

   **Stack:**
   - ¿Qué lenguaje(s) y framework(s) usan?
   - ¿Qué base de datos o almacenamiento?
   - ¿Alguna dependencia clave o restricción de versión?

   **Arquitectura:**
   - ¿Cuáles son los módulos o componentes principales del sistema?
   - ¿Hay límites o fronteras importantes entre ellos?

   **Convenciones:**
   - ¿Convenciones de naming, estructura de carpetas?
   - ¿Política de testing? ¿Cobertura esperada?
   - ¿Convención para commits o PRs?

   **Comandos:**
   - ¿Cómo se instalan las dependencias?
   - ¿Cómo se corre el proyecto en local?
   - ¿Cómo se corren los tests?
   - ¿Cómo se hace el build?

4. Con todas las respuestas, usa `refine_document()` via MCP para actualizar cada archivo:
   - `spec-native/PRODUCT.md`
   - `spec-native/STACK.md`
   - `spec-native/ARCHITECTURE.md`
   - `spec-native/CONVENTIONS.md`
   - `spec-native/COMMANDS.md`

5. Confirma al desarrollador mostrando qué archivos se actualizaron.

6. Sugiere el próximo paso:
   > "Contexto base definido. Próximo paso: usa `/spec-update` para refinar o el prompt `start_initiative` del MCP para crear tu primera spec."
