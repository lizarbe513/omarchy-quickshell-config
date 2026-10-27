//! Colores de una pantalla de desbloqueo. Cada uno puede ser automático o
//! fijo: el fondo automático es el color dominante del borde del dibujo (así
//! el dibujo se funde con la pantalla) y el texto automático, un blanco o un
//! negro suave que contraste con ese fondo (WCAG AA).
//! Recibe la imagen de `pantalla`/`vista`; devuelve colores a `composicion` y a la interfaz.

use image::DynamicImage;
use lizarbe_core::color::{luminance, parse_hex, to_hex, with_contrast};

pub type Rgb = (u8, u8, u8);

/// Contraste mínimo del texto sobre el fondo.
const CONTRASTE_TEXTO: f32 = 4.5;
const TEXTO_CLARO: Rgb = (0xe6, 0xe6, 0xea);
const TEXTO_OSCURO: Rgb = (0x1c, 0x1c, 0x1e);
/// Fondo cuando no hay dibujo del que sacarlo.
pub const FONDO_POR_DEFECTO: Rgb = (0x11, 0x11, 0x16);

/// Un color elegido por el usuario o calculado a partir del dibujo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tono {
    Automatico,
    Fijo(Rgb),
}

impl Tono {
    /// `"auto"` o `"#rrggbb"`, como se guarda en `desbloqueo.toml`.
    pub fn a_texto(self) -> String {
        match self {
            Tono::Automatico => "auto".into(),
            Tono::Fijo(c) => to_hex(c),
        }
    }

    pub fn desde_texto(s: &str) -> Tono {
        parse_hex(s).map_or(Tono::Automatico, Tono::Fijo)
    }
}

/// Fondo y texto ya resueltos (sin automáticos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colores {
    pub fondo: Rgb,
    pub texto: Rgb,
}

/// Resuelve los dos tonos con los píxeles del dibujo (si lo hay).
pub fn resolver(fondo: Tono, texto: Tono, dibujo: Option<&DynamicImage>) -> Colores {
    let fondo = match fondo {
        Tono::Fijo(c) => c,
        Tono::Automatico => dibujo.map_or(FONDO_POR_DEFECTO, fondo_del_borde),
    };
    let texto = match texto {
        Tono::Fijo(c) => c,
        Tono::Automatico => texto_para(fondo),
    };
    Colores { fondo, texto }
}

/// Texto legible sobre `fondo`.
pub fn texto_para(fondo: Rgb) -> Rgb {
    let base = if luminance(fondo) < 0.5 {
        TEXTO_CLARO
    } else {
        TEXTO_OSCURO
    };
    with_contrast(base, fondo, CONTRASTE_TEXTO)
}

/// Color dominante del contorno de la imagen.
fn fondo_del_borde(img: &DynamicImage) -> Rgb {
    let borde = pixeles_del_borde(img);
    dominantes(&borde, 3)
        .first()
        .map_or(FONDO_POR_DEFECTO, |c| c.0)
}

/// Colores que se ofrecen al elegir uno a mano: los dominantes del dibujo.
pub fn sugerencias(img: &DynamicImage, cuantas: usize) -> Vec<Rgb> {
    let small = img.thumbnail(64, 64).to_rgb8();
    let px: Vec<Rgb> = small.pixels().map(|p| (p[0], p[1], p[2])).collect();
    dominantes(&px, cuantas).into_iter().map(|c| c.0).collect()
}

/// Píxeles del marco exterior de la imagen reducida a 64×64 como mucho.
fn pixeles_del_borde(img: &DynamicImage) -> Vec<Rgb> {
    let small = img.thumbnail(64, 64).to_rgb8();
    let (w, h) = small.dimensions();
    small
        .enumerate_pixels()
        .filter(|(x, y, _)| *x == 0 || *y == 0 || *x + 1 == w || *y + 1 == h)
        .map(|(_, _, p)| (p[0], p[1], p[2]))
        .collect()
}

/// Colores dominantes (k-means determinista) de una lista de píxeles:
/// (color, cuántos píxeles), de más a menos frecuente.
pub fn dominantes(pixels: &[Rgb], k: usize) -> Vec<(Rgb, usize)> {
    if pixels.is_empty() || k == 0 {
        return vec![];
    }
    let k = k.min(pixels.len());
    let mut centers = centros_iniciales(pixels, k);
    let mut counts = vec![0usize; k];
    for _ in 0..12 {
        let mut sums = vec![[0f32; 3]; k];
        counts.iter_mut().for_each(|c| *c = 0);
        for p in pixels {
            let v = vector(p);
            let best = (0..k)
                .min_by(|&a, &b| dist(&centers[a], &v).total_cmp(&dist(&centers[b], &v)))
                .unwrap_or(0);
            counts[best] += 1;
            for c in 0..3 {
                sums[best][c] += v[c];
            }
        }
        for i in 0..k {
            if counts[i] > 0 {
                for c in 0..3 {
                    centers[i][c] = sums[i][c] / counts[i] as f32;
                }
            }
        }
    }
    let mut out: Vec<(Rgb, usize)> = centers
        .iter()
        .zip(&counts)
        .filter(|(_, n)| **n > 0)
        .map(|(c, n)| {
            (
                (c[0].round() as u8, c[1].round() as u8, c[2].round() as u8),
                *n,
            )
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.1));
    out
}

/// Centros iniciales: el píxel más cercano a la media y después, cada vez,
/// el más lejano a los ya elegidos (sin azar → resultado estable).
fn centros_iniciales(pixels: &[Rgb], k: usize) -> Vec<[f32; 3]> {
    let n = pixels.len() as f32;
    let mean = pixels.iter().fold([0f32; 3], |a, p| {
        let v = vector(p);
        [a[0] + v[0] / n, a[1] + v[1] / n, a[2] + v[2] / n]
    });
    let first = pixels
        .iter()
        .min_by(|a, b| dist(&vector(a), &mean).total_cmp(&dist(&vector(b), &mean)))
        .copied()
        .unwrap_or(pixels[0]);
    let mut centers: Vec<[f32; 3]> = vec![vector(&first)];
    while centers.len() < k {
        let lejania = |p: &Rgb| {
            centers
                .iter()
                .map(|c| dist(c, &vector(p)))
                .fold(f32::MAX, f32::min)
        };
        let far = pixels
            .iter()
            .max_by(|a, b| lejania(a).total_cmp(&lejania(b)))
            .copied()
            .unwrap_or(pixels[0]);
        centers.push(vector(&far));
    }
    centers
}

fn vector(p: &Rgb) -> [f32; 3] {
    [p.0 as f32, p.1 as f32, p.2 as f32]
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb as P, RgbImage};
    use lizarbe_core::color::contrast;

    /// Dibujo rojo en el centro sobre un marco verde.
    fn dibujo() -> DynamicImage {
        let mut img = RgbImage::from_pixel(100, 100, P([20, 120, 40]));
        for x in 20..80 {
            for y in 20..80 {
                img.put_pixel(x, y, P([200, 30, 30]));
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn el_fondo_automatico_sale_del_borde() {
        let c = resolver(Tono::Automatico, Tono::Automatico, Some(&dibujo()));
        assert_eq!(c.fondo, (20, 120, 40));
        assert!(contrast(c.texto, c.fondo) >= CONTRASTE_TEXTO);
    }

    #[test]
    fn los_tonos_fijos_se_respetan() {
        let c = resolver(Tono::Fijo((1, 2, 3)), Tono::Fijo((4, 5, 6)), None);
        assert_eq!(
            c,
            Colores {
                fondo: (1, 2, 3),
                texto: (4, 5, 6)
            }
        );
    }

    #[test]
    fn sin_dibujo_hay_un_fondo_oscuro_con_texto_claro() {
        let c = resolver(Tono::Automatico, Tono::Automatico, None);
        assert_eq!(c.fondo, FONDO_POR_DEFECTO);
        assert_eq!(c.texto, TEXTO_CLARO);
    }

    #[test]
    fn el_tono_se_guarda_y_se_lee() {
        assert_eq!(
            Tono::desde_texto(&Tono::Automatico.a_texto()),
            Tono::Automatico
        );
        let fijo = Tono::Fijo((0x12, 0xab, 0xef));
        assert_eq!(Tono::desde_texto(&fijo.a_texto()), fijo);
    }

    #[test]
    fn las_sugerencias_traen_los_dos_colores() {
        // Al reducir la imagen se mezclan los píxeles de la frontera: casi iguales basta.
        let s = sugerencias(&dibujo(), 2);
        let cerca = |c: Rgb| {
            s.iter()
                .any(|x| dist(&vector(x), &vector(&c)) < 30.0 * 30.0)
        };
        assert!(cerca((200, 30, 30)) && cerca((20, 120, 40)), "{s:?}");
    }
}
