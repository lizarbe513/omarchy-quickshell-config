//! Explorador de imágenes para elegir el dibujo: carpetas e imágenes
//! (PNG, JPG, WebP) de una carpeta, con accesos directos a los sitios
//! habituales. No sabe dibujarse: lo pinta `ui` y lo maneja `app`.

use std::path::{Path, PathBuf};

use lizarbe_core::paths as p;
use lizarbe_core::ui::fold;

const EXTENSIONES: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Subir,
    Carpeta,
    Imagen,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entrada {
    pub nombre: String,
    pub ruta: PathBuf,
    pub tipo: Tipo,
}

/// Un acceso directo: clave de texto (i18n) y carpeta.
pub struct Atajo {
    pub texto: &'static str,
    pub carpeta: PathBuf,
}

pub struct Explorador {
    pub carpeta: PathBuf,
    pub entradas: Vec<Entrada>,
    pub sel: usize,
    /// Ruta que se está escribiendo a mano (empieza con `/` o `~`).
    pub escrita: Option<String>,
}

impl Explorador {
    /// Abre `carpeta` (o la de imágenes si no existe).
    pub fn abrir(carpeta: &Path) -> Explorador {
        let carpeta = if carpeta.is_dir() {
            carpeta.to_path_buf()
        } else {
            carpeta_inicial()
        };
        let entradas = leer(&carpeta);
        Explorador {
            carpeta,
            entradas,
            sel: 0,
            escrita: None,
        }
    }

    pub fn ir_a(&mut self, carpeta: &Path) {
        *self = Explorador::abrir(carpeta);
    }

    /// Sube a la carpeta padre dejando seleccionada la que se dejó.
    pub fn subir(&mut self) {
        let Some(padre) = self.carpeta.parent().map(Path::to_path_buf) else {
            return;
        };
        let venia = self.carpeta.clone();
        self.ir_a(&padre);
        self.sel = self
            .entradas
            .iter()
            .position(|e| e.ruta == venia)
            .unwrap_or(0);
    }

    pub fn mover(&mut self, delta: isize) {
        if self.entradas.is_empty() {
            return;
        }
        let n = self.entradas.len() as isize;
        self.sel = (self.sel as isize + delta).clamp(0, n - 1) as usize;
    }

    pub fn actual(&self) -> Option<&Entrada> {
        self.entradas.get(self.sel)
    }

    /// Va a la ruta escrita: entra si es carpeta y la devuelve si es imagen.
    pub fn ir_a_escrita(&mut self) -> Result<Option<PathBuf>, ()> {
        let ruta = p::expand(self.escrita.take().unwrap_or_default().trim());
        if ruta.is_dir() {
            self.ir_a(&ruta);
            Ok(None)
        } else if ruta.is_file() && es_imagen(&ruta) {
            Ok(Some(ruta))
        } else {
            Err(())
        }
    }

    /// Abre la entrada elegida: entra en carpetas y devuelve la imagen elegida.
    pub fn activar(&mut self) -> Option<PathBuf> {
        let e = self.actual()?.clone();
        match e.tipo {
            Tipo::Subir => self.subir(),
            Tipo::Carpeta => self.ir_a(&e.ruta),
            Tipo::Imagen => return Some(e.ruta),
        }
        None
    }
}

/// La carpeta de imágenes del usuario (o su carpeta personal).
pub fn carpeta_inicial() -> PathBuf {
    dirs::picture_dir()
        .filter(|d| d.is_dir())
        .unwrap_or_else(p::home)
}

/// Accesos directos que existen en este equipo.
pub fn atajos() -> Vec<Atajo> {
    let candidatos = [
        ("archivos.imagenes", dirs::picture_dir()),
        ("archivos.descargas", dirs::download_dir()),
        (
            "archivos.fondos",
            Some(p::current_theme().join("backgrounds")),
        ),
        ("archivos.inicio", Some(p::home())),
    ];
    candidatos
        .into_iter()
        .filter_map(|(texto, c)| {
            c.filter(|d| d.is_dir())
                .map(|carpeta| Atajo { texto, carpeta })
        })
        .collect()
}

pub fn es_imagen(ruta: &Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONES.contains(&e.to_lowercase().as_str()))
}

/// Carpetas primero y después imágenes, por nombre; sin ocultos.
fn leer(carpeta: &Path) -> Vec<Entrada> {
    let mut out = vec![];
    if let Some(padre) = carpeta.parent() {
        out.push(Entrada {
            nombre: "..".into(),
            ruta: padre.to_path_buf(),
            tipo: Tipo::Subir,
        });
    }
    let Ok(rd) = std::fs::read_dir(carpeta) else {
        return out;
    };
    let mut resto: Vec<Entrada> = rd
        .flatten()
        .filter_map(|e| {
            let nombre = e.file_name().to_string_lossy().to_string();
            let ruta = e.path();
            let tipo = if ruta.is_dir() {
                Tipo::Carpeta
            } else if es_imagen(&ruta) {
                Tipo::Imagen
            } else {
                return None;
            };
            (!nombre.starts_with('.')).then_some(Entrada { nombre, ruta, tipo })
        })
        .collect();
    resto.sort_by_key(|e| (e.tipo == Tipo::Imagen, fold(&e.nombre)));
    out.extend(resto);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_carpetas_e_imagenes_y_navega() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("zeta")).unwrap();
        std::fs::create_dir(t.path().join(".oculta")).unwrap();
        for f in ["b.PNG", "a.jpg", "nota.txt"] {
            std::fs::write(t.path().join(f), "").unwrap();
        }
        let mut ex = Explorador::abrir(t.path());
        let nombres: Vec<&str> = ex.entradas.iter().map(|e| e.nombre.as_str()).collect();
        assert_eq!(nombres, ["..", "zeta", "a.jpg", "b.PNG"]);
        ex.sel = 1;
        assert_eq!(ex.activar(), None);
        assert_eq!(ex.carpeta, t.path().join("zeta"));
        ex.subir();
        assert_eq!(ex.actual().unwrap().nombre, "zeta");
        ex.mover(1);
        assert_eq!(ex.activar(), Some(t.path().join("a.jpg")));
        ex.mover(10);
        assert_eq!(ex.sel, 3);
        ex.escrita = Some(t.path().join("b.PNG").to_string_lossy().into());
        assert_eq!(ex.ir_a_escrita(), Ok(Some(t.path().join("b.PNG"))));
        ex.escrita = Some(t.path().join("zeta").to_string_lossy().into());
        assert_eq!(ex.ir_a_escrita(), Ok(None));
        assert_eq!(ex.carpeta, t.path().join("zeta"));
        ex.escrita = Some("/no/existe".into());
        assert_eq!(ex.ir_a_escrita(), Err(()));
    }
}
