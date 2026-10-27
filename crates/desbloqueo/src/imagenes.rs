//! Miniaturas de archivos de imagen dentro de la TUI (con sixel, kitty o
//! semibloques según la terminal). Las usa el explorador (`archivos`) para
//! enseñar la imagen resaltada. Se decodifican en un hilo para que la
//! interfaz no se congele con imágenes grandes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use image::DynamicImage;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

/// Lado máximo de una miniatura: de sobra para cualquier terminal.
const LADO_MINIATURA: u32 = 1280;

pub enum Entry {
    Loading,
    Ready(Box<StatefulProtocol>),
    Failed,
}

struct Loaded {
    path: PathBuf,
    result: Option<DynamicImage>,
}

pub struct Images {
    picker: Picker,
    entries: HashMap<PathBuf, Entry>,
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
}

impl Images {
    pub fn new(picker: Picker) -> Images {
        let (tx, rx) = channel();
        Images {
            picker,
            entries: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Pide cargar una imagen si aún no se pidió.
    pub fn request(&mut self, path: &Path) {
        if self.entries.contains_key(path) {
            return;
        }
        self.entries.insert(path.to_path_buf(), Entry::Loading);
        let tx = self.tx.clone();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = image::ImageReader::open(&path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
                .map(|img| img.thumbnail(LADO_MINIATURA, LADO_MINIATURA));
            let _ = tx.send(Loaded { path, result });
        });
    }

    /// Recoge las imágenes que ya se decodificaron; devuelve sus rutas.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        let mut done = vec![];
        while let Ok(l) = self.rx.try_recv() {
            let entry = match l.result {
                Some(img) => Entry::Ready(Box::new(self.picker.new_resize_protocol(img))),
                None => Entry::Failed,
            };
            self.entries.insert(l.path.clone(), entry);
            done.push(l.path);
        }
        done
    }

    pub fn entry(&mut self, path: &Path) -> Option<&mut Entry> {
        self.entries.get_mut(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb as P, RgbImage};

    fn esperar(imgs: &mut Images) -> Vec<PathBuf> {
        for _ in 0..100 {
            let done = imgs.poll();
            if !done.is_empty() {
                return done;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        vec![]
    }

    #[test]
    fn loads_in_the_background() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.png");
        RgbImage::from_pixel(200, 100, P([10, 20, 30]))
            .save(&p)
            .unwrap();
        let mut imgs = Images::new(Picker::halfblocks());
        imgs.request(&p);
        assert!(matches!(imgs.entry(&p), Some(Entry::Loading)));
        assert_eq!(esperar(&mut imgs), vec![p.clone()]);
        assert!(matches!(imgs.entry(&p), Some(Entry::Ready(_))));
    }

    #[test]
    fn a_broken_image_fails_without_crashing() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("rota.png");
        std::fs::write(&p, "no es una imagen").unwrap();
        let mut imgs = Images::new(Picker::halfblocks());
        imgs.request(&p);
        esperar(&mut imgs);
        assert!(matches!(imgs.entry(&p), Some(Entry::Failed)));
    }
}
