//! Las pantallas de desbloqueo guardadas. Cada una es una carpeta
//! `desbloqueo-<nombre>` dentro de los temas del usuario
//! (`~/.config/omarchy/themes/`), con lo que Omarchy necesita para listarla
//! en Estilo → Desbloqueo y aplicarla (`omarchy-plymouth-set-by-theme`):
//!
//! - `unlock.png`: el dibujo ya escalado, que Plymouth pinta a su tamaño real.
//! - `preview-unlock.png`: la miniatura del selector de Omarchy.
//! - `colors.toml`: `background` y `foreground` de la pantalla.
//!
//! y, para volver a editarla, `dibujo.png` (el original) y `desbloqueo.toml`.
//! Recibe lo que edita `app`; usa `composicion` y `paleta` para las imágenes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use image::DynamicImage;
use lizarbe_core::color::to_hex;
use lizarbe_core::fsutil::write_atomic;
use lizarbe_core::ui::fold;
use serde::{Deserialize, Serialize};

use crate::composicion::{self, ALTURA_INICIAL, LIENZO, Piezas};
use crate::paleta::{self, Colores, Tono};

/// Prefijo de las carpetas de esta herramienta.
pub const PREFIJO: &str = "desbloqueo-";
const AJUSTES: &str = "desbloqueo.toml";
const DIBUJO: &str = "dibujo.png";
/// Lado máximo con el que se guarda el dibujo original.
const LADO_MAX_ORIGINAL: u32 = 4096;

/// Una pantalla de desbloqueo, guardada o en edición.
#[derive(Debug, Clone, PartialEq)]
pub struct Pantalla {
    /// Carpeta donde está guardada (`None` si aún no se guardó).
    pub carpeta: Option<String>,
    pub nombre: String,
    /// Imagen de la que sale el dibujo.
    pub dibujo: Option<PathBuf>,
    /// Alto del dibujo en % del alto de la pantalla.
    pub altura: u8,
    pub fondo: Tono,
    pub texto: Tono,
}

/// Lo que se guarda en `desbloqueo.toml`.
#[derive(Serialize, Deserialize)]
struct Ajustes {
    nombre: String,
    altura: u8,
    fondo: String,
    texto: String,
}

impl Pantalla {
    pub fn nueva(nombre: &str) -> Pantalla {
        Pantalla {
            carpeta: None,
            nombre: nombre.into(),
            dibujo: None,
            altura: ALTURA_INICIAL,
            fondo: Tono::Automatico,
            texto: Tono::Automatico,
        }
    }

    /// Una copia sin guardar con otro nombre.
    pub fn copia(&self, nombre: &str) -> Pantalla {
        Pantalla {
            carpeta: None,
            nombre: nombre.into(),
            ..self.clone()
        }
    }
}

/// Nombre de carpeta para un nombre visible: `Mi gato!` → `desbloqueo-mi-gato`.
pub fn carpeta_para(nombre: &str) -> String {
    let mut s = String::new();
    for c in fold(nombre).chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-');
    format!("{PREFIJO}{}", if s.is_empty() { "sin-nombre" } else { s })
}

/// Las pantallas guardadas en `temas`, por nombre.
pub fn listar(temas: &Path) -> Vec<Pantalla> {
    let Ok(rd) = std::fs::read_dir(temas) else {
        return vec![];
    };
    let mut out: Vec<Pantalla> = rd
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(PREFIJO))
        .filter_map(|e| leer(&e.path()).ok())
        .collect();
    out.sort_by_key(|p| fold(&p.nombre));
    out
}

/// Lee una pantalla de su carpeta.
pub fn leer(carpeta: &Path) -> Result<Pantalla> {
    let texto = std::fs::read_to_string(carpeta.join(AJUSTES))
        .with_context(|| format!("no hay {AJUSTES} en {}", carpeta.display()))?;
    let a: Ajustes = toml::from_str(&texto).context("desbloqueo.toml no se puede leer")?;
    let dibujo = carpeta.join(DIBUJO);
    Ok(Pantalla {
        carpeta: Some(
            carpeta
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        ),
        nombre: a.nombre,
        dibujo: dibujo.is_file().then_some(dibujo),
        altura: a.altura,
        fondo: Tono::desde_texto(&a.fondo),
        texto: Tono::desde_texto(&a.texto),
    })
}

/// Guarda la pantalla en `temas` (renombra su carpeta si cambió el nombre) y
/// devuelve el nombre de la carpeta. `pantalla_real` es la resolución del
/// monitor, para la que se escala `unlock.png`.
pub fn guardar(
    temas: &Path,
    p: &Pantalla,
    piezas: &Piezas,
    pantalla_real: (u32, u32),
) -> Result<String> {
    let Some(origen) = &p.dibujo else {
        bail!("falta elegir el dibujo");
    };
    let dibujo =
        image::open(origen).with_context(|| format!("no se puede abrir {}", origen.display()))?;
    let destino = preparar_carpeta(temas, p)?;
    let dir = temas.join(&destino);
    escribir_imagenes(&dir, p, &dibujo, piezas, pantalla_real)?;
    let ajustes = Ajustes {
        nombre: p.nombre.clone(),
        altura: p.altura,
        fondo: p.fondo.a_texto(),
        texto: p.texto.a_texto(),
    };
    write_atomic(&dir.join(AJUSTES), &toml::to_string(&ajustes)?)?;
    Ok(destino)
}

/// Crea (o renombra) la carpeta de destino y devuelve su nombre.
fn preparar_carpeta(temas: &Path, p: &Pantalla) -> Result<String> {
    let destino = carpeta_para(&p.nombre);
    let dir = temas.join(&destino);
    let propia = p.carpeta.as_deref() == Some(destino.as_str());
    if dir.exists() && !propia {
        bail!("ya hay otra pantalla llamada así");
    }
    match &p.carpeta {
        Some(vieja) if !propia => {
            std::fs::rename(temas.join(vieja), &dir).context("no se pudo renombrar la carpeta")?
        }
        _ => std::fs::create_dir_all(&dir).context("no se pudo crear la carpeta")?,
    }
    Ok(destino)
}

/// `dibujo.png`, `unlock.png`, `preview-unlock.png` y `colors.toml`.
fn escribir_imagenes(
    dir: &Path,
    p: &Pantalla,
    dibujo: &DynamicImage,
    piezas: &Piezas,
    pantalla_real: (u32, u32),
) -> Result<()> {
    let original = dir.join(DIBUJO);
    if p.dibujo.as_deref() != Some(original.as_path()) {
        dibujo
            .thumbnail(LADO_MAX_ORIGINAL, LADO_MAX_ORIGINAL)
            .save_with_format(&original, image::ImageFormat::Png)?;
    }
    let colores = paleta::resolver(p.fondo, p.texto, Some(dibujo));
    let unlock = composicion::escalar(dibujo, pantalla_real, p.altura, piezas.alto_caja());
    unlock.save_with_format(dir.join("unlock.png"), image::ImageFormat::Png)?;
    let para_lienzo = composicion::escalar(dibujo, LIENZO, p.altura, piezas.alto_caja());
    composicion::componer(&para_lienzo, piezas, colores, LIENZO)
        .save_with_format(dir.join("preview-unlock.png"), image::ImageFormat::Png)?;
    write_atomic(&dir.join("colors.toml"), &colors_toml(colores))?;
    Ok(())
}

fn colors_toml(c: Colores) -> String {
    format!(
        "# Pantalla de desbloqueo creada con lizarbe-desbloqueo.\nbackground = \"{}\"\nforeground = \"{}\"\n",
        to_hex(c.fondo),
        to_hex(c.texto)
    )
}

/// Borra la carpeta de una pantalla (solo si es de esta herramienta).
pub fn borrar(temas: &Path, carpeta: &str) -> Result<()> {
    let dir = temas.join(carpeta);
    if !carpeta.starts_with(PREFIJO) || carpeta.contains('/') || !dir.join(AJUSTES).is_file() {
        bail!("{carpeta} no es una pantalla de desbloqueo");
    }
    std::fs::remove_dir_all(&dir).context("no se pudo borrar la carpeta")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn con_dibujo(dir: &Path, nombre: &str) -> Pantalla {
        let img = dir.join("gato.png");
        RgbImage::from_pixel(300, 200, Rgb([40, 50, 60]))
            .save(&img)
            .unwrap();
        Pantalla {
            dibujo: Some(img),
            ..Pantalla::nueva(nombre)
        }
    }

    #[test]
    fn el_nombre_de_carpeta_es_seguro() {
        assert_eq!(carpeta_para("Mi Gato!"), "desbloqueo-mi-gato");
        assert_eq!(carpeta_para("  Ñandú  azul "), "desbloqueo-nandu-azul");
        assert_eq!(carpeta_para("../.."), "desbloqueo-sin-nombre");
    }

    #[test]
    fn guarda_lo_que_omarchy_necesita_y_se_vuelve_a_leer() {
        let t = tempfile::tempdir().unwrap();
        let mut p = con_dibujo(t.path(), "Gato");
        p.fondo = Tono::Fijo((1, 2, 3));
        let carpeta = guardar(t.path(), &p, &Piezas::simples(), (1920, 1080)).unwrap();
        let dir = t.path().join(&carpeta);
        for f in [
            "unlock.png",
            "preview-unlock.png",
            "colors.toml",
            "dibujo.png",
            AJUSTES,
        ] {
            assert!(dir.join(f).is_file(), "falta {f}");
        }
        let colores = std::fs::read_to_string(dir.join("colors.toml")).unwrap();
        assert!(colores.contains("background = \"#010203\""));
        let leida = leer(&dir).unwrap();
        assert_eq!(leida.nombre, "Gato");
        assert_eq!(leida.fondo, p.fondo);
        assert_eq!(leida.dibujo, Some(dir.join(DIBUJO)));
        assert_eq!(listar(t.path()).len(), 1);
    }

    #[test]
    fn renombrar_mueve_la_carpeta_y_no_pisa_otra() {
        let t = tempfile::tempdir().unwrap();
        let piezas = Piezas::simples();
        let a = con_dibujo(t.path(), "Uno");
        let carpeta = guardar(t.path(), &a, &piezas, (1920, 1080)).unwrap();
        let mut leida = leer(&t.path().join(&carpeta)).unwrap();
        leida.nombre = "Dos".into();
        assert_eq!(
            guardar(t.path(), &leida, &piezas, (1920, 1080)).unwrap(),
            "desbloqueo-dos"
        );
        assert!(!t.path().join(&carpeta).exists());
        assert!(
            guardar(
                t.path(),
                &con_dibujo(t.path(), "Dos"),
                &piezas,
                (1920, 1080)
            )
            .is_err()
        );
    }

    #[test]
    fn solo_borra_carpetas_propias() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("catppuccin")).unwrap();
        assert!(borrar(t.path(), "catppuccin").is_err());
        let carpeta = guardar(
            t.path(),
            &con_dibujo(t.path(), "X"),
            &Piezas::simples(),
            (1920, 1080),
        )
        .unwrap();
        borrar(t.path(), &carpeta).unwrap();
        assert!(listar(t.path()).is_empty());
    }

    #[test]
    fn sin_dibujo_no_se_guarda() {
        let t = tempfile::tempdir().unwrap();
        assert!(
            guardar(
                t.path(),
                &Pantalla::nueva("x"),
                &Piezas::simples(),
                (1920, 1080)
            )
            .is_err()
        );
    }
}
