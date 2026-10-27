//! Vista previa en vivo: compone la pantalla en un hilo cada vez que cambia
//! algo (dibujo, tamaño o colores) y la deja lista para dibujarla en la TUI.
//! Si llegan varios pedidos seguidos, solo se queda con el último.
//! Recibe pedidos de `app`; usa `composicion` y `paleta`; la pinta `ui`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use image::DynamicImage;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::composicion::{self, LIENZO, Piezas};
use crate::paleta::{self, Colores, Rgb, Tono};

/// Lado máximo con el que se decodifica el dibujo para la vista previa.
const LADO_DIBUJO: u32 = 2048;
/// Tamaño de la vista previa (la mitad del lienzo: de sobra para la terminal).
const VISTA: (u32, u32) = (LIENZO.0 / 2, LIENZO.1 / 2);
/// Cuántos colores del dibujo se ofrecen al elegir uno.
const SUGERENCIAS: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct Pedido {
    pub dibujo: PathBuf,
    pub altura: u8,
    pub fondo: Tono,
    pub texto: Tono,
}

pub enum Estado {
    /// No hay dibujo todavía.
    Vacia,
    Calculando,
    Lista(Box<StatefulProtocol>),
    Fallo,
}

struct Hecho {
    numero: u64,
    resultado: Option<(DynamicImage, Colores, Vec<Rgb>)>,
}

/// Dibujo decodificado por última vez (para no volver a leerlo del disco).
type Cache = Arc<Mutex<Option<(PathBuf, Arc<DynamicImage>)>>>;

pub struct Vista {
    picker: Picker,
    piezas: Arc<Piezas>,
    cache: Cache,
    tx: Sender<Hecho>,
    rx: Receiver<Hecho>,
    numero: u64,
    ultimo: Option<Pedido>,
    pub estado: Estado,
    /// Colores resueltos de la última composición (los automáticos ya calculados).
    pub colores: Option<Colores>,
    /// Colores dominantes del dibujo, para el selector de color.
    pub sugerencias: Vec<Rgb>,
}

impl Vista {
    pub fn new(picker: Picker, piezas: Arc<Piezas>) -> Vista {
        let (tx, rx) = channel();
        Vista {
            picker,
            piezas,
            cache: Arc::new(Mutex::new(None)),
            tx,
            rx,
            numero: 0,
            ultimo: None,
            estado: Estado::Vacia,
            colores: None,
            sugerencias: vec![],
        }
    }

    /// Pide componer de nuevo si el pedido cambió.
    pub fn pedir(&mut self, pedido: Option<Pedido>) {
        if pedido == self.ultimo {
            return;
        }
        self.ultimo = pedido.clone();
        self.numero += 1;
        let Some(pedido) = pedido else {
            self.estado = Estado::Vacia;
            self.colores = None;
            self.sugerencias.clear();
            return;
        };
        self.estado = Estado::Calculando;
        let (tx, numero) = (self.tx.clone(), self.numero);
        let (piezas, cache) = (self.piezas.clone(), self.cache.clone());
        std::thread::spawn(move || {
            let resultado = componer(&pedido, &piezas, &cache);
            let _ = tx.send(Hecho { numero, resultado });
        });
    }

    /// Recoge la última composición terminada; `true` si cambió algo.
    pub fn recoger(&mut self) -> bool {
        let mut cambio = false;
        while let Ok(h) = self.rx.try_recv() {
            if h.numero != self.numero {
                continue;
            }
            cambio = true;
            match h.resultado {
                Some((img, colores, sugerencias)) => {
                    self.estado = Estado::Lista(Box::new(self.picker.new_resize_protocol(img)));
                    self.colores = Some(colores);
                    self.sugerencias = sugerencias;
                }
                None => self.estado = Estado::Fallo,
            }
        }
        cambio
    }
}

fn componer(
    p: &Pedido,
    piezas: &Piezas,
    cache: &Cache,
) -> Option<(DynamicImage, Colores, Vec<Rgb>)> {
    let dibujo = decodificar(&p.dibujo, cache)?;
    let colores = paleta::resolver(p.fondo, p.texto, Some(&dibujo));
    let escalado = composicion::escalar(&dibujo, LIENZO, p.altura, piezas.alto_caja());
    let lienzo = composicion::componer(&escalado, piezas, colores, LIENZO);
    let vista = DynamicImage::ImageRgba8(lienzo).thumbnail(VISTA.0, VISTA.1);
    Some((vista, colores, paleta::sugerencias(&dibujo, SUGERENCIAS)))
}

fn decodificar(ruta: &PathBuf, cache: &Cache) -> Option<Arc<DynamicImage>> {
    if let Some((r, img)) = cache.lock().ok()?.as_ref()
        && r == ruta
    {
        return Some(img.clone());
    }
    let img = Arc::new(image::open(ruta).ok()?.thumbnail(LADO_DIBUJO, LADO_DIBUJO));
    *cache.lock().ok()? = Some((ruta.clone(), img.clone()));
    Some(img)
}
