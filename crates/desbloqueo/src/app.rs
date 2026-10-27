//! Estado de la herramienta y sus acciones: la lista de pantallas guardadas,
//! la que se está editando, el diálogo abierto y los avisos. Las teclas y el
//! ratón llegan por `entrada`; lo dibuja `ui`; guarda con `pantalla` y
//! aplica con `omarchy-plymouth-set-by-theme` (pide sudo en la terminal).

use std::path::{Path, PathBuf};
use std::process::{Command as Proc, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lizarbe_core::mouse::Clicks;
use lizarbe_core::term::{CANCELLED, Command};
use lizarbe_core::theme::{Palette, PaletteWatcher};
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use crate::archivos::{self, Explorador};
use crate::composicion::{ALTURA_MAX, ALTURA_MIN, LIENZO, Piezas};
use crate::i18n::{t, tf};
use crate::imagenes::Images;
use crate::paleta::Tono;
use crate::pantalla::{self, Pantalla};
use crate::vista::{Pedido, Vista};

/// Cuánto dura un aviso en la barra inferior.
const AVISO: Duration = Duration::from_secs(6);
/// Paso del tamaño del dibujo con ← →.
const PASO_ALTURA: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foco {
    Lista,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Campo {
    Nombre,
    Dibujo,
    Altura,
    Fondo,
    Texto,
}

impl Campo {
    pub const TODOS: [Campo; 5] = [
        Campo::Nombre,
        Campo::Dibujo,
        Campo::Altura,
        Campo::Fondo,
        Campo::Texto,
    ];
}

pub enum Dialogo {
    Nombre(String),
    Explorador(Explorador),
    /// Elegir el fondo o el texto: automático, un color del dibujo o uno escrito.
    Color {
        campo: Campo,
        sel: usize,
        escrito: Option<String>,
    },
    Borrar,
}

/// Zonas donde se puede hacer clic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Fila de la lista; la última es «Nueva».
    Pantalla(usize),
    Campo(Campo),
    Entrada(usize),
    Atajo(usize),
    OpcionColor(usize),
}

pub struct Aviso {
    pub texto: String,
    pub bien: bool,
    desde: Instant,
}

pub struct App {
    pub temas: PathBuf,
    sandbox: bool,
    pub pantallas: Vec<Pantalla>,
    /// Fila elegida de la lista (`pantallas.len()` = «Nueva»).
    pub sel: usize,
    pub editando: Pantalla,
    /// Cómo está guardada la que se edita (`None` si es nueva).
    guardada: Option<Pantalla>,
    pub foco: Foco,
    pub campo: Campo,
    pub dialogo: Option<Dialogo>,
    pub vista: Vista,
    pub imagenes: Images,
    piezas: Arc<Piezas>,
    pub aviso: Option<Aviso>,
    /// Carpeta de la pantalla que está puesta en el arranque.
    pub activa: Option<String>,
    pub paleta: Palette,
    vigia: Option<PaletteWatcher>,
    pantalla_real: (u32, u32),
    comando: Option<Command>,
    aplicando: Option<String>,
    /// Acción que espera una segunda pulsación para descartar cambios.
    pub confirmar_descarte: Option<Descarte>,
    pub hits: Vec<(Rect, Hit)>,
    pub hover: Option<Hit>,
    pub clicks: Clicks<Hit>,
    quit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Descarte {
    Ir(usize),
    Salir,
}

impl App {
    /// `sandbox`: se trabaja en `temas` sin tocar el sistema (no se aplica nada).
    pub fn new(temas: PathBuf, sandbox: bool, picker: Picker, colores: &Path) -> App {
        let plymouth = lizarbe_core::paths::omarchy().join("default/plymouth");
        let piezas = Arc::new(Piezas::cargar(&plymouth).unwrap_or_else(|_| Piezas::simples()));
        let mut app = App {
            temas,
            sandbox,
            pantallas: vec![],
            sel: 0,
            editando: Pantalla::nueva(""),
            guardada: None,
            foco: Foco::Lista,
            campo: Campo::Nombre,
            dialogo: None,
            vista: Vista::new(picker.clone(), piezas.clone()),
            imagenes: Images::new(picker),
            piezas,
            aviso: None,
            activa: None,
            paleta: Palette::load(colores),
            vigia: (!sandbox).then(|| PaletteWatcher::new(colores)),
            pantalla_real: if sandbox {
                LIENZO
            } else {
                resolucion_del_monitor()
            },
            comando: None,
            aplicando: None,
            confirmar_descarte: None,
            hits: vec![],
            hover: None,
            clicks: Clicks::default(),
            quit: false,
        };
        if !sandbox {
            app.activa = pantalla_activa();
        }
        app.recargar();
        app.cargar(if app.pantallas.is_empty() { 0 } else { app.sel });
        app
    }

    /// Abre la pantalla de esa carpeta (opción `--editar`).
    pub fn abrir(&mut self, carpeta: &str) {
        match self
            .pantallas
            .iter()
            .position(|p| p.carpeta.as_deref() == Some(carpeta))
        {
            Some(i) => {
                self.cargar(i);
                self.foco = Foco::Editor;
            }
            None => self.avisar(tf("aviso.no_existe", &[("nombre", carpeta)]), false),
        }
    }

    // ------------------------------------------------------------ consultas

    pub fn cambios(&self) -> bool {
        match &self.guardada {
            Some(g) => *g != self.editando,
            None => self.editando.dibujo.is_some(),
        }
    }

    pub fn es_activa(&self) -> bool {
        self.guardada.is_some() && self.activa.is_some() && self.activa == self.editando.carpeta
    }

    /// Modo de prueba (`--config-dir`): no se mira ni se cambia el arranque.
    pub fn en_prueba(&self) -> bool {
        self.sandbox
    }

    pub fn salir_pedido(&self) -> bool {
        self.quit
    }

    // ------------------------------------------------------------ lista

    fn recargar(&mut self) {
        self.pantallas = pantalla::listar(&self.temas);
    }

    /// Carga la fila `i` en el editor (`pantallas.len()` = una nueva).
    fn cargar(&mut self, i: usize) {
        self.sel = i.min(self.pantallas.len());
        match self.pantallas.get(self.sel) {
            Some(p) => {
                self.editando = p.clone();
                self.guardada = Some(p.clone());
            }
            None => {
                self.editando = Pantalla::nueva(&self.nombre_libre(&t("nueva.nombre")));
                self.guardada = None;
            }
        }
        self.confirmar_descarte = None;
        self.actualizar_vista();
    }

    /// Ir a otra fila; si hay cambios sin guardar pide repetir para descartarlos.
    pub fn seleccionar(&mut self, i: usize) {
        let i = i.min(self.pantallas.len());
        if i == self.sel {
            return;
        }
        if self.cambios() && self.confirmar_descarte != Some(Descarte::Ir(i)) {
            self.confirmar_descarte = Some(Descarte::Ir(i));
            self.avisar(t("aviso.sin_guardar"), false);
            return;
        }
        self.cargar(i);
    }

    pub fn mover(&mut self, delta: isize) {
        let n = self.pantallas.len() as isize + 1;
        self.seleccionar((self.sel as isize + delta).clamp(0, n - 1) as usize);
    }

    pub fn nueva(&mut self) {
        self.seleccionar(self.pantallas.len());
        if self.sel == self.pantallas.len() {
            self.foco = Foco::Editor;
            self.campo = Campo::Dibujo;
        }
    }

    pub fn duplicar(&mut self) {
        if self.guardada.is_none() {
            return;
        }
        let nombre = self.nombre_libre(&tf("nueva.copia", &[("nombre", &self.editando.nombre)]));
        self.editando = self.editando.copia(&nombre);
        self.guardada = None;
        self.sel = self.pantallas.len();
        self.foco = Foco::Editor;
        self.avisar(t("aviso.duplicada"), true);
    }

    /// `base`, o `base 2`, `base 3`… si ya hay una carpeta con ese nombre.
    fn nombre_libre(&self, base: &str) -> String {
        let ocupado = |n: &str| self.temas.join(pantalla::carpeta_para(n)).exists();
        let mut nombre = base.to_string();
        let mut i = 2;
        while ocupado(&nombre) {
            nombre = format!("{base} {i}");
            i += 1;
        }
        nombre
    }

    pub fn pedir_salir(&mut self) {
        if self.cambios() && self.confirmar_descarte != Some(Descarte::Salir) {
            self.confirmar_descarte = Some(Descarte::Salir);
            self.avisar(t("aviso.salir_sin_guardar"), false);
            return;
        }
        self.quit = true;
    }

    // ------------------------------------------------------------ edición

    pub fn cambiar_altura(&mut self, mas: bool) {
        let a = self.editando.altura;
        self.editando.altura = if mas {
            a.saturating_add(PASO_ALTURA).min(ALTURA_MAX)
        } else {
            a.saturating_sub(PASO_ALTURA).max(ALTURA_MIN)
        };
        self.actualizar_vista();
    }

    pub fn poner_nombre(&mut self, nombre: &str) {
        let nombre = nombre.trim();
        if !nombre.is_empty() {
            self.editando.nombre = nombre.to_string();
        }
    }

    pub fn poner_dibujo(&mut self, ruta: PathBuf) {
        self.editando.dibujo = Some(ruta);
        self.actualizar_vista();
    }

    pub fn poner_tono(&mut self, campo: Campo, tono: Tono) {
        match campo {
            Campo::Fondo => self.editando.fondo = tono,
            Campo::Texto => self.editando.texto = tono,
            _ => return,
        }
        self.actualizar_vista();
    }

    pub fn tono(&self, campo: Campo) -> Tono {
        if campo == Campo::Texto {
            self.editando.texto
        } else {
            self.editando.fondo
        }
    }

    fn actualizar_vista(&mut self) {
        let e = &self.editando;
        self.vista.pedir(e.dibujo.clone().map(|dibujo| Pedido {
            dibujo,
            altura: e.altura,
            fondo: e.fondo,
            texto: e.texto,
        }));
    }

    /// Abre el diálogo que corresponde al campo.
    pub fn editar_campo(&mut self, campo: Campo) {
        self.foco = Foco::Editor;
        self.campo = campo;
        self.dialogo = match campo {
            Campo::Nombre => Some(Dialogo::Nombre(self.editando.nombre.clone())),
            Campo::Dibujo => Some(Dialogo::Explorador(Explorador::abrir(
                &self.carpeta_del_dibujo(),
            ))),
            Campo::Altura => None,
            Campo::Fondo | Campo::Texto => Some(Dialogo::Color {
                campo,
                sel: 0,
                escrito: None,
            }),
        };
    }

    /// Dónde abrir el explorador: junto al dibujo actual si es del usuario.
    fn carpeta_del_dibujo(&self) -> PathBuf {
        self.editando
            .dibujo
            .as_ref()
            .and_then(|d| d.parent())
            .filter(|d| !d.starts_with(&self.temas))
            .map(Path::to_path_buf)
            .unwrap_or_else(archivos::carpeta_inicial)
    }

    // ------------------------------------------------------------ guardar y aplicar

    /// Guarda la pantalla; `true` si quedó guardada.
    pub fn guardar(&mut self) -> bool {
        if self.editando.dibujo.is_none() {
            self.avisar(t("aviso.falta_dibujo"), false);
            return false;
        }
        match pantalla::guardar(
            &self.temas,
            &self.editando,
            &self.piezas,
            self.pantalla_real,
        ) {
            Ok(carpeta) => {
                self.recargar();
                let i = self
                    .pantallas
                    .iter()
                    .position(|p| p.carpeta.as_deref() == Some(&carpeta));
                self.cargar(i.unwrap_or(self.pantallas.len()));
                self.avisar(t("aviso.guardada"), true);
                true
            }
            Err(e) => {
                self.avisar(
                    tf("aviso.error_guardar", &[("error", &e.to_string())]),
                    false,
                );
                false
            }
        }
    }

    /// Guarda si hace falta y la pone en el arranque.
    pub fn aplicar(&mut self) {
        if (self.cambios() || self.guardada.is_none()) && !self.guardar() {
            return;
        }
        let Some(carpeta) = self.editando.carpeta.clone() else {
            return;
        };
        if self.sandbox {
            self.avisar(t("aviso.sandbox"), false);
            return;
        }
        self.comando = Some(Command {
            program: "omarchy-plymouth-set-by-theme".into(),
            args: vec![carpeta.clone()],
        });
        self.aplicando = Some(carpeta);
    }

    pub fn pedir_borrar(&mut self) {
        if self.guardada.is_some() {
            self.dialogo = Some(Dialogo::Borrar);
        }
    }

    pub fn borrar(&mut self) {
        self.dialogo = None;
        let Some(carpeta) = self.editando.carpeta.clone() else {
            return;
        };
        match pantalla::borrar(&self.temas, &carpeta) {
            Ok(()) => {
                let i = self.sel;
                self.recargar();
                self.cargar(i.min(self.pantallas.len().saturating_sub(1)));
                self.avisar(tf("aviso.borrada", &[("nombre", &carpeta)]), true);
            }
            Err(e) => self.avisar(e.to_string(), false),
        }
    }

    pub fn tomar_comando(&mut self) -> Option<Command> {
        self.comando.take()
    }

    pub fn tras_comando(&mut self, resultado: Result<bool, String>) {
        let carpeta = self.aplicando.take().unwrap_or_default();
        match resultado {
            Ok(true) => {
                self.activa = Some(carpeta);
                self.avisar(t("aviso.aplicada"), true);
            }
            Ok(false) => self.avisar(t("aviso.error_aplicar"), false),
            Err(e) if e == CANCELLED => self.avisar(t("aviso.cancelado"), false),
            Err(e) => self.avisar(tf("aviso.error_lanzar", &[("error", &e)]), false),
        }
    }

    // ------------------------------------------------------------ avisos y tiempo

    pub fn avisar(&mut self, texto: String, bien: bool) {
        self.aviso = Some(Aviso {
            texto,
            bien,
            desde: Instant::now(),
        });
    }

    pub fn tick(&mut self) {
        self.vista.recoger();
        self.imagenes.poll();
        if let Some(v) = &mut self.vigia {
            v.poll(&mut self.paleta);
        }
        if self
            .aviso
            .as_ref()
            .is_some_and(|a| a.desde.elapsed() > AVISO)
        {
            self.aviso = None;
            self.confirmar_descarte = None;
        }
    }
}

/// Resolución del monitor enfocado (Plymouth pinta a la resolución real).
fn resolucion_del_monitor() -> (u32, u32) {
    let salida = Proc::new("hyprctl")
        .args(["monitors", "-j"])
        .stderr(Stdio::null())
        .output();
    let Ok(salida) = salida else {
        return LIENZO;
    };
    let monitores: Vec<serde_json::Value> =
        serde_json::from_slice(&salida.stdout).unwrap_or_default();
    let tam =
        |m: &serde_json::Value| Some((m["width"].as_u64()? as u32, m["height"].as_u64()? as u32));
    monitores
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))
        .or(monitores.first())
        .and_then(tam)
        .unwrap_or(LIENZO)
}

/// La pantalla de arranque puesta ahora, según `omarchy-plymouth-current`.
fn pantalla_activa() -> Option<String> {
    let salida = Proc::new("omarchy-plymouth-current")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let nombre = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    (!nombre.is_empty()).then_some(nombre)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(dir: &Path) -> App {
        App::new(
            dir.to_path_buf(),
            true,
            Picker::halfblocks(),
            &dir.join("colors.toml"),
        )
    }

    fn con_dibujo(app: &mut App, dir: &Path) {
        let img = dir.join("gato.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([9, 9, 9]))
            .save(&img)
            .unwrap();
        app.poner_dibujo(img);
    }

    #[test]
    fn sin_pantallas_empieza_con_una_nueva() {
        let t = tempfile::tempdir().unwrap();
        let a = app(t.path());
        assert_eq!(a.sel, 0);
        assert!(a.pantallas.is_empty());
        assert!(!a.cambios());
    }

    #[test]
    fn guardar_la_anade_a_la_lista_y_borrar_la_quita() {
        let t = tempfile::tempdir().unwrap();
        let mut a = app(t.path());
        assert!(!a.guardar());
        con_dibujo(&mut a, t.path());
        assert!(a.cambios());
        assert!(a.guardar());
        assert_eq!(a.pantallas.len(), 1);
        assert!(!a.cambios());
        a.borrar();
        assert!(a.pantallas.is_empty());
    }

    #[test]
    fn cambiar_de_fila_con_cambios_pide_confirmar() {
        let t = tempfile::tempdir().unwrap();
        let mut a = app(t.path());
        con_dibujo(&mut a, t.path());
        a.guardar();
        a.cambiar_altura(true);
        a.mover(1);
        assert_eq!(a.sel, 0);
        a.mover(1);
        assert_eq!(a.sel, 1);
    }

    #[test]
    fn en_modo_de_prueba_no_se_aplica_nada() {
        let t = tempfile::tempdir().unwrap();
        let mut a = app(t.path());
        con_dibujo(&mut a, t.path());
        a.aplicar();
        assert!(a.tomar_comando().is_none());
        assert_eq!(a.pantallas.len(), 1);
    }

    #[test]
    fn el_tamano_no_se_sale_de_sus_limites() {
        let t = tempfile::tempdir().unwrap();
        let mut a = app(t.path());
        for _ in 0..30 {
            a.cambiar_altura(true);
        }
        assert_eq!(a.editando.altura, ALTURA_MAX);
        for _ in 0..30 {
            a.cambiar_altura(false);
        }
        assert_eq!(a.editando.altura, ALTURA_MIN);
    }
}
