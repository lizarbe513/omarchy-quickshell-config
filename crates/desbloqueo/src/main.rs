//! Crear tema de desbloqueo — crea pantallas de desbloqueo (la que pide la
//! contraseña del disco al encender) con un dibujo propio. Se guardan como
//! carpetas en los temas del usuario, así Omarchy las ofrece en
//! Estilo → Desbloqueo, y se pueden aplicar desde aquí.

mod app;
mod archivos;
mod composicion;
mod dialogos;
mod entrada;
mod i18n;
mod imagenes;
mod paleta;
mod pantalla;
mod ui;
mod vista;

use anyhow::Result;
use lizarbe_core::cli::{self, Opt};
use lizarbe_core::paths as p;

use crate::app::App;
use crate::i18n::t;

const OPTS: &[Opt] = &[Opt::value("--editar", "<carpeta>", "cli.editar")];

fn main() -> Result<()> {
    let args = cli::parse(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), OPTS, t);
    let sandbox = args.config_dir.is_some();
    let temas = match &args.config_dir {
        Some(d) => d.join("themes"),
        None => p::home().join(".config/omarchy/themes"),
    };
    let _ = std::fs::create_dir_all(&temas);
    // Pregunta a la terminal qué gráficos admite (sixel, kitty…); si no responde, semibloques.
    let picker = if sandbox {
        ratatui_image::picker::Picker::halfblocks()
    } else {
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks())
    };
    let colores = p::current_theme().join("colors.toml");
    let mut app = App::new(temas, sandbox, picker, &colores);
    if let Some(carpeta) = args.value("--editar") {
        app.abrir(carpeta);
    }
    lizarbe_core::term::run(&mut app, &t("app.name"))
}
