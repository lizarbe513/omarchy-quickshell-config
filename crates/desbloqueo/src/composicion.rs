//! La pantalla de desbloqueo tal como la dibuja Plymouth en Omarchy
//! (`default/plymouth/omarchy.script`): el dibujo centrado a su tamaño real
//! y, 40 px por debajo, la caja de contraseña con el candado a su izquierda y
//! los puntos dentro. Imita a `omarchy-plymouth-preview` para la vista previa.
//! Recibe el dibujo y los colores; produce las imágenes que guarda `pantalla`
//! y la vista previa que muestra `vista`.

use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

use crate::paleta::{Colores, Rgb};

/// Lienzo de `preview-unlock.png` (el mismo que usa Omarchy).
pub const LIENZO: (u32, u32) = (1920, 1080);
/// Alto del dibujo en % del alto de la pantalla.
pub const ALTURA_MIN: u8 = 15;
pub const ALTURA_MAX: u8 = 70;
pub const ALTURA_INICIAL: u8 = 40;
/// Distancia entre el dibujo y la caja (fija en `omarchy.script`).
const SEPARACION_CAJA: u32 = 40;
/// Aire bajo la caja para que no toque el borde de la pantalla.
const MARGEN: u32 = 24;
/// Ancho máximo del dibujo respecto al de la pantalla.
const ANCHO_MAX_PCT: u32 = 90;
/// Medidas del candado y los puntos (las de `omarchy.script`).
const CANDADO_BASE: (u32, u32) = (84, 96);
const LADO_PUNTO: u32 = 7;
const PUNTOS: u32 = 4;

/// Las piezas de la caja de contraseña.
pub struct Piezas {
    caja: RgbaImage,
    candado: RgbaImage,
    punto: RgbaImage,
}

impl Piezas {
    /// Las piezas reales de Omarchy (`entry.png`, `lock.png`, `bullet.png`).
    pub fn cargar(dir: &Path) -> Result<Piezas, String> {
        let abrir = |nombre: &str| {
            image::open(dir.join(nombre))
                .map(|i| i.to_rgba8())
                .map_err(|e| format!("{nombre}: {e}"))
        };
        Ok(Piezas {
            caja: abrir("entry.png")?,
            candado: abrir("lock.png")?,
            punto: abrir("bullet.png")?,
        })
    }

    /// Piezas lisas del mismo tamaño, para cuando no está Omarchy (pruebas).
    pub fn simples() -> Piezas {
        let opaco = |w, h| RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        let mut caja = RgbaImage::new(286, 48);
        for (x, y, p) in caja.enumerate_pixels_mut() {
            if x == 0 || y == 0 || x == 285 || y == 47 {
                *p = Rgba([255, 255, 255, 255]);
            }
        }
        Piezas {
            caja,
            candado: opaco(CANDADO_BASE.0, CANDADO_BASE.1),
            punto: opaco(LADO_PUNTO, LADO_PUNTO),
        }
    }

    pub fn alto_caja(&self) -> u32 {
        self.caja.height()
    }
}

/// Alto en píxeles del dibujo para una pantalla de `alto_pantalla`: el
/// porcentaje pedido, sin pasar del que deja sitio a la caja por debajo.
pub fn alto_del_dibujo(alto_pantalla: u32, porcentaje: u8, alto_caja: u32) -> u32 {
    let pct = porcentaje.clamp(ALTURA_MIN, ALTURA_MAX) as u32;
    let pedido = alto_pantalla * pct / 100;
    let maximo = alto_pantalla.saturating_sub(2 * (SEPARACION_CAJA + alto_caja + MARGEN));
    pedido.min(maximo).max(1)
}

/// El dibujo escalado como lo guardará `unlock.png`.
pub fn escalar(
    dibujo: &DynamicImage,
    pantalla: (u32, u32),
    porcentaje: u8,
    alto_caja: u32,
) -> RgbaImage {
    let (w, h) = dibujo.dimensions();
    let mut alto = alto_del_dibujo(pantalla.1, porcentaje, alto_caja);
    let mut ancho = (w as u64 * alto as u64 / h.max(1) as u64) as u32;
    let ancho_max = pantalla.0 * ANCHO_MAX_PCT / 100;
    if ancho > ancho_max {
        ancho = ancho_max;
        alto = (h as u64 * ancho as u64 / w.max(1) as u64) as u32;
    }
    dibujo
        .resize_exact(ancho.max(1), alto.max(1), FilterType::Lanczos3)
        .to_rgba8()
}

/// La pantalla completa: fondo, dibujo, caja, candado y puntos.
pub fn componer(
    dibujo: &RgbaImage,
    piezas: &Piezas,
    colores: Colores,
    lienzo: (u32, u32),
) -> RgbaImage {
    let (w, h) = lienzo;
    let (f, t) = (colores.fondo, colores.texto);
    let mut out = RgbaImage::from_pixel(w, h, Rgba([f.0, f.1, f.2, 255]));
    let dibujo_y = centrar(h, dibujo.height());
    imageops::overlay(&mut out, dibujo, centrar(w, dibujo.width()), dibujo_y);

    let caja = tenir(&piezas.caja, t);
    let caja_x = centrar(w, caja.width());
    let caja_y = dibujo_y + dibujo.height() as i64 + SEPARACION_CAJA as i64;
    let centro_caja = caja_y + caja.height() as i64 / 2;
    imageops::overlay(&mut out, &caja, caja_x, caja_y);

    let alto_candado = caja.height() * 8 / 10;
    let ancho_candado = CANDADO_BASE.0 * alto_candado / CANDADO_BASE.1;
    let candado = imageops::resize(
        &tenir(&piezas.candado, t),
        ancho_candado,
        alto_candado,
        FilterType::Triangle,
    );
    let candado_x = caja_x - ancho_candado as i64 - 15;
    imageops::overlay(
        &mut out,
        &candado,
        candado_x,
        centro_caja - alto_candado as i64 / 2,
    );

    let punto = imageops::resize(
        &tenir(&piezas.punto, t),
        LADO_PUNTO,
        LADO_PUNTO,
        FilterType::Triangle,
    );
    for i in 0..PUNTOS as i64 {
        imageops::overlay(&mut out, &punto, caja_x + 20 + i * 12, centro_caja - 4);
    }
    out
}

/// Coordenada para centrar `tam` en `total` (negativa si no cabe).
fn centrar(total: u32, tam: u32) -> i64 {
    (total as i64 - tam as i64) / 2
}

/// La misma imagen con todo el color cambiado por `c` (se conserva el alfa),
/// como `magick +level-colors` en `omarchy-plymouth-preview`.
fn tenir(img: &RgbaImage, c: Rgb) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        *p = Rgba([c.0, c.1, c.2, p[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORES: Colores = Colores {
        fondo: (10, 10, 10),
        texto: (250, 250, 250),
    };

    fn dibujo(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([200, 0, 0, 255])))
    }

    #[test]
    fn el_alto_respeta_el_porcentaje_y_el_sitio_de_la_caja() {
        assert_eq!(alto_del_dibujo(1080, 40, 48), 432);
        // 70 % de 1080 = 756, pero la caja necesita 2·(40+48+24) = 224 → 856: cabe.
        assert_eq!(alto_del_dibujo(1080, 70, 48), 756);
        // Pantalla baja: manda el sitio de la caja.
        assert_eq!(alto_del_dibujo(400, 70, 48), 176);
        assert_eq!(alto_del_dibujo(1080, 1, 48), 1080 * ALTURA_MIN as u32 / 100);
    }

    #[test]
    fn un_dibujo_muy_ancho_no_se_sale_de_la_pantalla() {
        let img = escalar(&dibujo(4000, 500), LIENZO, 70, 48);
        assert_eq!(img.width(), 1920 * ANCHO_MAX_PCT / 100);
        assert_eq!(img.height(), 216);
    }

    #[test]
    fn la_caja_queda_dentro_de_la_pantalla() {
        let piezas = Piezas::simples();
        let escalado = escalar(&dibujo(800, 800), LIENZO, ALTURA_MAX, piezas.alto_caja());
        let out = componer(&escalado, &piezas, COLORES, LIENZO);
        assert_eq!(out.dimensions(), LIENZO);
        // El dibujo en el centro y el borde de la caja con el color del texto.
        assert_eq!(out.get_pixel(960, 540), &Rgba([200, 0, 0, 255]));
        let caja_y = (1080 - escalado.height()) / 2 + escalado.height() + SEPARACION_CAJA;
        assert!(caja_y + 48 + MARGEN <= 1080);
        assert_eq!(out.get_pixel(960, caja_y), &Rgba([250, 250, 250, 255]));
        assert_eq!(out.get_pixel(5, 5), &Rgba([10, 10, 10, 255]));
    }
}
