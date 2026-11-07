# Validador científico externo de Physure

**Estado:** propuesta para aprobación  
**Fecha:** 2026-10-30  
**Nombre recomendado del futuro repositorio privado:** `physure-crucible`  
**Ubicación acordada:** `D:\Projects\physure-crucible`

## Intención

Physure necesita una autoridad de conformidad independiente de sus pruebas internas. El
validador tratará las distribuciones candidatas de PHS y los crates Rust como productos de
caja negra, reproducirá problemas científicos de nivel universitario y decidirá si la
configuración predeterminada es suficientemente veraz para publicarse.

Una discrepancia en una capacidad declarada estable —resultado físico falso, cálculo
matemático incorrecto, dimensión o unidad inválida, incertidumbre incorrecta o aceptación
silenciosa de una entrada imposible— bloqueará el release. El mismo repositorio mantendrá
casos de frontera para orientar desarrollo experimental, pero esos casos no se confundirán
con la conformidad ya prometida.

El repositorio validador será cerrado, estará junto a este monorepo bajo `D:\Projects` y no
formará parte del workspace de Physure. Este documento vive temporalmente en Physure para
acordar el diseño antes de crear el nuevo repositorio.

## Decisiones acordadas

- El resultado de conformidad es un gate obligatorio para todo release.
- Se valida la configuración predeterminada; una opción no predeterminada no puede rescatar
  un fallo del comportamiento que recibe un usuario nuevo.
- El sistema se prueba exclusivamente mediante superficies públicas.
- El validador cubre tanto el lenguaje PHS como el consumo público de Rust.
- La arquitectura será híbrida: Python coordina y calcula referencias independientes; PHS se
  ejecuta como producto externo; pequeños programas Rust consumen los crates publicados.
- La primera etapa evalúa ecuaciones físicas ya proporcionadas. No exige que Physure derive
  simbólicamente el modelo ni resuelva todavía EDO o PDE.
- El currículo crecerá por las áreas habituales de ingeniería: cinemática y dinámica,
  electricidad y magnetismo, termodinámica y fluidos, mecánica y dinámica de medios
  continuos, y análisis de circuitos y electrónica.
- La primera vertical de conformidad será mecánica clásica.

## Objetivos

1. Detectar respuestas científicamente incorrectas aunque Rust y PHS coincidan entre sí.
2. Detectar divergencias entre PHS y la API pública Rust para el mismo problema.
3. Verificar unidades, dimensiones, conversiones, constantes, signos, dominios e
   incertidumbres, además del valor nominal.
4. Validar PHS como lenguaje científico: sintaxis, semántica, errores, código de salida y
   reproducibilidad.
5. Producir por candidato un informe auditable con la referencia, tolerancia, resultado y
   evidencia cruda de cada ejecución.
6. Conservar casos experimentales fallidos como especificaciones ejecutables que puedan
   ascender posteriormente a conformidad.
7. Probar que el propio validador puede detectar una implementación deliberadamente errónea;
   una batería que siempre queda verde no es evidencia.

## No objetivos de la primera etapa

- Derivación simbólica de ecuaciones.
- Integración general de EDO, resolución de PDE, mallado o elección de discretizaciones.
- Benchmarks de rendimiento o carga.
- Validación exhaustiva de todas las disciplinas en el primer hito.
- Uso de módulos privados, fixtures internos o funciones de test de Physure.
- Sustituir las pruebas unitarias del monorepo.
- Publicar el código, los vectores adversariales o la totalidad de los reportes. Cualquier
  apertura futura será otra decisión.

## Antiobjetivos

- Usar una salida de Physure como oráculo para otra salida de Physure.
- Aplicar una tolerancia global suficientemente amplia para que “todo pase”.
- Convertir resultados redondeados de presentación en la referencia numérica.
- Marcar un fallo de conformidad como esperado sólo para desbloquear un release.
- Añadir un framework propio cuando `pytest`, procesos del sistema y archivos TOML basten.

## Modelo de confianza y aislamiento

El validador recibirá una **descripción de candidato** con identidad inmutable:

- ruta y hash del ejecutable `phs` que se pretende publicar;
- paquete/crate candidato y versión o hash que usarán los consumidores Rust;
- commit, plataforma, arquitectura y versiones de toolchain;
- variables de entorno relevantes.

No ejecutará código desde el árbol fuente de Physure ni importará el workspace. Para Rust,
cada prueba compilará un consumidor mínimo en un directorio temporal contra el paquete
candidato y sólo podrá nombrar elementos públicos. Para PHS, el runner invocará el ejecutable
como lo haría un usuario, sin archivos de configuración adicionales salvo que el caso sea
precisamente una prueba explícita de configuración.

El candidato Rust de pre-release será el `.crate` producido por `cargo package -p physure`,
no el directorio `physure-core`. El validador extraerá ese paquete en un temporal y los crates
consumidores lo resolverán como dependencia `physure` desde esa copia empaquetada. De este
modo se comprueba exactamente qué entraría al registro antes de publicarlo. Las dependencias
quedarán fijadas por lockfile y disponibles offline en el entorno de CI.

El entorno limpio usará un directorio de trabajo temporal, directorios de usuario/config/cache
vacíos, locale UTF-8 fijo, red deshabilitada y una allowlist mínima de variables necesarias
para ejecutar el sistema y los toolchains. El reporte guardará esa allowlist y cualquier
excepción. Ni la configuración personal del desarrollador ni un `physure.conf` del checkout
podrán entrar accidentalmente en la ejecución.

Las fuentes científicas y tablas normativas necesarias se guardarán versionadas en el repo
privado. La ejecución de conformidad no dependerá de red ni de contenido web mutable.

## Contrato público inicial

La frontera de conformidad de una versión será lo que esa versión documente como público y
estable en sus guías, `--help` y API Rust con semver. Una capacidad marcada explícitamente
experimental entra primero en `cases/experimental`; que hoy funcione por accidente no la
convierte en promesa.

La primera vertical necesita únicamente este subconjunto estable:

- **PHS:** literales de cantidad, unidades compuestas, `+/-`, aritmética, potencias,
  conversiones con `=>`, comparaciones y `assert`, ejecución de un archivo y diagnóstico de
  errores con código de salida no cero.
- **Rust (`physure_core` desde el paquete `physure`):** construcción de cantidades exactas y
  con desviación estándar, aritmética, potencia, conversión, lectura de media/desviación
  estándar/unidad y errores mediante `PhysureResult`.

Antes de añadir casos, el implementador escribirá dos probes mínimos que compilen y ejecuten
ese inventario contra el candidato empaquetado. Si documentación y artefacto discrepan, es un
fallo del candidato y no se adapta silenciosamente el validador.

Para entradas inválidas, PHS debe terminar con código distinto de cero y un diagnóstico para
el usuario, sin panic ni backtrace como interfaz normal. La API Rust debe devolver `Err`; un
panic del consumidor se considera fallo. Las pruebas de lenguaje pueden exigir una categoría
o fragmento estable del diagnóstico, pero las pruebas científicas ordinarias no fijarán toda
la redacción humana.

## Arquitectura elegida

### Coordinador Python

Python 3.12 o posterior ejecutará `pytest`, descubrirá los casos, calculará o cargará la
referencia, invocará PHS y los consumidores Rust, comparará resultados y escribirá el
reporte. Se usarán primero `decimal`, `fractions`, `math`, `subprocess`, `tempfile` y
`tomllib` de la biblioteca estándar. `mpmath` será la única dependencia numérica inicial
cuando una referencia trascendental requiera más precisión que `f64`; no se incorporará
SciPy hasta que un caso concreto lo justifique.

No habrá un DSL de tests. Cada caso tendrá un `case.toml` pequeño y los archivos ejecutables
que realmente necesite.

### Adaptador PHS

El runner ejecutará scripts `.phs` con el binario candidato, impondrá un timeout y conservará
stdout, stderr y código de salida. En la primera vertical, el propio script terminará con
`assert(...)` públicos generados a partir de la referencia y tolerancia del caso. Así se evita
depender del formato humano de presentación para decidir conformidad.

Las pruebas específicamente dedicadas a representación y diagnósticos sí comprobarán el
texto visible, pero sólo los fragmentos que formen parte del contrato del lenguaje. Si PHS
incorpora una salida estructurada pública, el adaptador podrá adoptarla después sin cambiar
la definición científica de los casos.

### Consumidores Rust

Cada familia de casos tendrá un crate consumidor mínimo. Recibirá entradas por argumentos o
un fixture generado, llamará exclusivamente a la API pública, y emitirá una línea de resultado
con un prefijo estable controlado por el validador. El coordinador compilará y ejecutará el
crate contra el paquete candidato. Los warnings, fallos de compilación, panics y códigos de
salida no cero serán evidencia del caso, no errores ocultos del harness.

### Oráculos independientes

Cada referencia declarará una de estas procedencias:

- identidad matemática exacta calculada con enteros, `Fraction` o `Decimal`;
- derivación analítica incluida y revisable en el caso;
- constante o definición normativa con edición, tabla, página y fecha;
- cálculo de alta precisión con algoritmo y precisión explícitos;
- dataset experimental versionado, cuando las fases futuras lo requieran.

Una biblioteca externa puede ayudar a verificar una referencia, pero su nombre o su valor
por defecto no reemplazan la procedencia. Cuando sea práctico, el valor se verificará por dos
métodos independientes durante la creación del caso y se almacenará después como snapshot.

## Organización propuesta del nuevo repositorio

```text
physure-crucible/
  pyproject.toml
  README.md
  validator/
    candidate.py
    run_phs.py
    run_rust.py
    report.py
  references/
    sources.toml
  cases/
    conformance/
      mechanics/
    experimental/
      mathematics/
      physics/
  tests/
    test_validator.py
  reports/
```

Esta estructura es deliberadamente pequeña. Los módulos se separan sólo por los tres límites
reales: describir el candidato, ejecutar cada superficie y reportar.

Cada caso será autocontenido:

```text
cases/conformance/mechanics/constant-acceleration/
  case.toml
  model.md
  program.phs
  rust-consumer/
```

`model.md` explica la derivación, supuestos y fuente. `case.toml` contiene sólo metadatos
ejecutables: identificador, entradas, salidas esperadas, unidades, superficies aplicables,
tolerancias y referencias. El runner no intentará convertir TOML en un lenguaje de
programación; un caso que necesite lógica propia tendrá un módulo Python local y explícito.

## Contrato de un caso científico

Todo caso de conformidad debe declarar:

- identificador estable y área;
- pregunta física y nivel universitario;
- modelo, supuestos y dominio de validez;
- cantidades de entrada con unidades e incertidumbres;
- resultado nominal, dimensión y unidad esperados;
- comportamiento esperado ante entradas inválidas;
- fuente de verdad independiente;
- tolerancia absoluta y/o relativa con justificación;
- superficies que deben pasar: PHS, Rust o ambas;
- variantes metamórficas, cuando correspondan;
- clasificación `conformance` o `experimental`.

La comparación numérica será
`abs(actual - reference) <= max(abs_tol, rel_tol * abs(reference))`. Cada caso escogerá sus
tolerancias desde su error analítico, representación `f64`, condicionamiento y, más adelante,
error estadístico. Unidades, dimensiones, aceptación/rechazo y códigos de salida son contratos
discretos y no reciben tolerancia.

Las variantes metamórficas prueban que cambiar de unidades, escalar entradas o reformular una
identidad no cambia la física. Son especialmente valiosas porque no dependen de copiar un
número dorado distinto para cada variante.

### Incertidumbre predeterminada

El gate probará el default real de Physure: `propagation_mode = correlated`. `+/-` y
`Quantity::with_uncertainty` representan desviación estándar absoluta. La misma medición
conserva linaje, por lo que `x - x` cancela; mediciones creadas independientemente se combinan
según propagación analítica de primer orden. Los primeros casos contrastarán media y desviación
estándar almacenadas, no los dígitos redondeados de presentación. El formato GUM, cuando sea
parte de un release, tendrá casos de lenguaje separados para no confundir exactitud numérica
con presentación.

## Conformidad frente a frontera experimental

`cases/conformance` sólo contiene capacidades que el release afirma soportar. No se permiten
`xfail`, reintentos que oculten intermitencia ni listas de fallos conocidos. Un fallo bloquea.

`cases/experimental` usa el mismo formato y produce la misma evidencia, pero sus fallos no
cambian el código de salida del gate. Cada discrepancia registra la primera versión observada,
resultado esperado, resultado real y limitación inferida. Promover un caso a conformidad es
un cambio revisado que elimina cualquier expectativa de fallo.

## Primera vertical: mecánica clásica

El primer paquete debe contener entre 10 y 15 casos profundos, no cientos de ejemplos
superficiales. Cubrirá:

1. movimiento rectilíneo uniforme y uniformemente acelerado;
2. caída libre y lanzamiento de proyectil sin resistencia;
3. segunda ley de Newton con fuerzas vectoriales o componentes equivalentes;
4. trabajo y energía cinética;
5. conservación de momento en colisiones idealizadas;
6. movimiento circular y fuerza centrípeta;
7. torque, momento de inercia y energía rotacional;
8. resorte ideal y frecuencia natural mediante la fórmula analítica dada;
9. propagación predeterminada de incertidumbre en al menos energía cinética y período;
10. rechazo de sumas dimensionalmente incompatibles y de parámetros fuera del dominio.

Cada familia incluirá, donde tenga sentido, SI, una combinación no SI, extremos de magnitud y
una variante físicamente inválida. El caso no pedirá a Physure descubrir la ecuación: la
ecuación forma parte del script o consumidor.

Después del piloto, el mismo contrato se extenderá a circuitos y electrónica, termodinámica y
fluidos, electricidad y magnetismo, y finalmente medios continuos. El orden exacto después de
mecánica puede cambiar sin alterar esta arquitectura.

## Gate de release e informe

Una ejecución de release falla si ocurre cualquiera de estos eventos en conformidad:

- diferencia fuera de tolerancia;
- unidad o dimensión incorrecta;
- PHS y Rust divergen para el mismo contrato;
- una entrada inválida se acepta silenciosamente;
- una entrada válida falla, hace panic, excede timeout o devuelve salida no interpretable;
- el consumidor Rust no compila contra la API pública candidata;
- el binario usa una configuración distinta de la predeterminada sin que el caso lo solicite.

El reporte conservará por caso: candidato y hashes, plataforma, referencia, tolerancia,
resultado, duración, comando sanitizado, stdout/stderr y clasificación del fallo. Habrá una
salida breve para CI y un artefacto estructurado completo. Los reportes no se publicarán
automáticamente desde el repositorio cerrado.

Las plataformas obligatorias serán las mismas que Physure declare soportadas para ese
release; el gate sólo queda satisfecho cuando cada plataforma requerida reporta conformidad.
La matriz actual de `core-release.yml` es Linux x86_64/aarch64, macOS x86_64/aarch64 y Windows
x86_64. El agregador tomará esta lista del manifiesto del candidato y rechazará una matriz
incompleta; no mantendrá una segunda lista silenciosamente divergente.

## Verificación del propio validador

El repositorio tendrá pruebas pequeñas para el comparador de tolerancias, timeouts, captura de
errores, selección `conformance`/`experimental` y generación del reporte. Además, CI ejecutará
un candidato falso o fixture mutante con un resultado conocido incorrecto y exigirá que el
gate falle. Esto comprueba el camino completo sin introducir hooks especiales en Physure.

Los cambios de referencias, tolerancias y promoción de casos deben verse claramente en review;
no se regenerarán snapshots científicos de forma automática durante una ejecución normal.

## Criterios de aceptación del primer hito

El primer hito se considera completo cuando:

1. el runner acepta una descripción inmutable de candidato;
2. los casos de mecánica ejecutan PHS y consumidores Rust fuera del árbol de Physure;
3. cada caso enlaza su derivación, fuente y tolerancia;
4. un candidato correcto produce un reporte completo y código de salida cero;
5. una mutación deliberada produce código no cero y evidencia legible;
6. un fallo experimental queda reportado sin desbloquear ni bloquear falsamente conformidad;
7. la ejecución es reproducible sin red;
8. ningún test depende de símbolos privados o fixtures del monorepo.

## Superficies afectadas cuando se implemente

- **Nuevo repositorio privado:** runner, referencias, casos, consumidores Rust, pruebas y CI.
- **Physure:** ninguna modificación funcional para el primer hito. La integración futura sólo
  entregará artefactos candidatos y esperará el resultado del gate.
- **Proceso de release:** no se publica hasta recibir conformidad en todas las plataformas
  soportadas.

Si una superficie pública actual no permite observar un resultado sin analizar texto humano,
eso se registrará como hallazgo de DX. Añadir una salida estructurada a Physure requerirá su
propio diseño y no se colará dentro del validador.

## Alternativas consideradas

### Sólo Python

Era la opción más pequeña y ofrecía buenos oráculos, pero no reproducía la experiencia de un
consumidor real de la API Rust ni detectaba roturas de compilación pública.

### Sólo Rust

Validaba bien tipos y API, pero reducía la independencia de la auditoría y hacía más costoso
mantener referencias exactas y reportes científicos.

### Híbrido — elegido

Python mantiene independiente la verdad científica; PHS y crates Rust se ejercitan como
productos externos. Cuesta un adaptador adicional, pero permite atribuir fallos a ciencia,
lenguaje o API pública sin duplicar toda la batería.

## Decisiones dejadas al implementador

- Proveedor de CI.
- Forma exacta del reporte estructurado (`json` o `junit.xml`), siempre que preserve la
  evidencia exigida.
- Agrupación de crates consumidores para minimizar tiempo de compilación sin perder el
  aislamiento de la API pública.
- Orden de las disciplinas posteriores a mecánica.
