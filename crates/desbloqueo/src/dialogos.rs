//! Ventanas emergentes sobre la pantalla principal: el nombre, el
//! explorador de imágenes (con miniatura de la imagen resaltada), el
//! selector de color y la confirmación de borrar. Marco recto de 1 celda.
//! Lee el diálogo abierto de `app` y anota sus zonas de clic.

use lizarbe_core::color::to_hex;
use lizarbe_core::ui::{centered, fg, pad, put, truncate};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use ratatui_image::{Resize, StatefulImage};

use crate::app::{App, Campo, Dialogo, Hit};
use crate::archivos::{self, Explorador, Tipo};
use crate::i18n::{t, tf};
use crate::imagenes::Entry;
use crate::paleta::Tono;
use crate::ui::{muestra_de_color, seccion};

pub fn dibujar(app: &mut App, f: &mut Frame, area: Rect) {
    match app.dialogo.take() {
        Some(Dialogo::Nombre(texto)) => {
            nombre(app, f, area, &texto);
            app.dialogo = Some(Dialogo::Nombre(texto));
        }
        Some(Dialogo::Explorador(ex)) => {
            explorador(app, f, area, &ex);
            app.dialogo = Some(Dialogo::Explorador(ex));
        }
        Some(Dialogo::Color {
            campo,
            sel,
            escrito,
        }) => {
            color(app, f, area, campo, sel, escrito.as_deref());
            app.dialogo = Some(Dialogo::Color {
                campo,
                sel,
                escrito,
            });
        }
        Some(Dialogo::Borrar) => {
            borrar(app, f, area);
            app.dialogo = Some(Dialogo::Borrar);
        }
        None => {}
    }
    if app.dialogo.is_some() {
        // Por debajo del diálogo no se puede hacer clic.
        app.hits
            .retain(|(_, h)| matches!(h, Hit::Entrada(_) | Hit::Atajo(_) | Hit::OpcionColor(_)));
    }
}

/// Marco recto con título; devuelve el interior con una celda de aire.
fn marco(app: &App, f: &mut Frame, r: Rect, titulo: &str) -> Rect {
    let p = &app.paleta;
    f.render_widget(Clear, r);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(fg(p.accent))
        .style(Style::new().bg(p.bg))
        .title(Span::styled(
            format!(" {titulo} "),
            fg(p.bright).add_modifier(Modifier::BOLD),
        ));
    let dentro = block.inner(r);
    f.render_widget(block, r);
    Rect::new(
        dentro.x + 1,
        dentro.y + 1,
        dentro.width.saturating_sub(2),
        dentro.height.saturating_sub(2),
    )
}

/// Línea de atajos de un diálogo: tecla en acento y acción atenuada.
fn ayuda(app: &App, f: &mut Frame, x: u16, y: u16, atajos: &[(&str, String)]) {
    let p = &app.paleta;
    let mut spans = vec![];
    for (i, (k, d)) in atajos.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("    "));
        }
        spans.push(Span::styled(k.to_string(), fg(p.accent)));
        spans.push(Span::styled(format!(" {d}"), fg(p.muted)));
    }
    put(f, x, y, spans);
}

fn nombre(app: &App, f: &mut Frame, area: Rect, texto: &str) {
    let r = marco(app, f, centered(area, 56, 8), &t("dialogo.nombre"));
    let p = &app.paleta;
    let w = r.width as usize - 1;
    let mostrado = truncate_izquierda(texto, w);
    put(
        f,
        r.x,
        r.y,
        vec![
            Span::styled(mostrado, fg(p.bright)),
            Span::styled("▏", fg(p.accent)),
        ],
    );
    put(
        f,
        r.x,
        r.y + 1,
        vec![Span::styled("─".repeat(r.width as usize), fg(p.accent))],
    );
    ayuda(
        app,
        f,
        r.x,
        r.bottom() - 1,
        &[("⏎", t("atajo.aceptar")), ("Esc", t("atajo.cancelar"))],
    );
}

/// Los últimos `w` caracteres (para rutas y textos largos que se escriben).
fn truncate_izquierda(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        return s.to_string();
    }
    format!("…{}", s.chars().skip(n - w + 1).collect::<String>())
}

fn explorador(app: &mut App, f: &mut Frame, area: Rect, ex: &Explorador) {
    let caja = centered(area, area.width * 9 / 10, area.height * 9 / 10);
    let r = marco(app, f, caja, &t("dialogo.dibujo"));
    let p = app.paleta.clone();
    let ruta = match &ex.escrita {
        Some(texto) => vec![
            Span::styled(
                truncate_izquierda(texto, r.width as usize - 1),
                fg(p.bright),
            ),
            Span::styled("▏", fg(p.accent)),
        ],
        None => vec![Span::styled(
            truncate_izquierda(&ex.carpeta.to_string_lossy(), r.width as usize),
            fg(p.muted),
        )],
    };
    put(f, r.x, r.y, ruta);
    atajos_de_carpeta(app, f, Rect::new(r.x, r.y + 1, r.width, 1));
    let ancho_lista = (r.width * 2 / 5).max(24);
    let lista = Rect::new(r.x, r.y + 3, ancho_lista, r.height.saturating_sub(5));
    entradas(app, f, lista, ex);
    let vista = Rect::new(
        r.x + ancho_lista + 2,
        r.y + 3,
        r.width.saturating_sub(ancho_lista + 2),
        lista.height,
    );
    miniatura(app, f, vista, ex);
    ayuda(
        app,
        f,
        r.x,
        r.bottom() - 1,
        &[
            ("⏎", t("atajo.elegir")),
            ("⌫", t("atajo.subir")),
            ("1-4", t("atajo.atajos")),
            ("/", t("atajo.ruta")),
            ("Esc", t("atajo.cancelar")),
        ],
    );
}

fn atajos_de_carpeta(app: &mut App, f: &mut Frame, r: Rect) {
    let p = app.paleta.clone();
    let mut x = r.x;
    for (i, a) in archivos::atajos().iter().enumerate() {
        let texto = t(a.texto);
        let spans = vec![
            Span::styled(format!("{}", i + 1), fg(p.accent)),
            Span::styled(format!(" {texto}"), fg(p.fg)),
        ];
        let w = put(f, x, r.y, spans);
        app.hits.push((Rect::new(x, r.y, w, 1), Hit::Atajo(i)));
        x += w + 4;
        if x >= r.right() {
            break;
        }
    }
}

fn entradas(app: &mut App, f: &mut Frame, r: Rect, ex: &Explorador) {
    let p = app.paleta.clone();
    if ex.entradas.len() <= 1 {
        put(
            f,
            r.x,
            r.y,
            vec![Span::styled(t("archivos.vacia"), fg(p.muted))],
        );
    }
    let filas = r.height as usize;
    let desde = ex.sel.saturating_sub(filas.saturating_sub(1));
    for (n, (i, e)) in ex
        .entradas
        .iter()
        .enumerate()
        .skip(desde)
        .take(filas)
        .enumerate()
    {
        let y = r.y + n as u16;
        let elegida = i == ex.sel;
        let (icono, color) = match e.tipo {
            Tipo::Subir | Tipo::Carpeta => ("󰉋 ", p.muted),
            Tipo::Imagen => ("󰋩 ", p.accent),
        };
        let estilo = if elegida || app.hover == Some(Hit::Entrada(i)) {
            fg(p.bright).add_modifier(Modifier::BOLD)
        } else {
            fg(p.fg)
        };
        let ancho = r.width as usize - 4;
        put(
            f,
            r.x,
            y,
            vec![
                Span::styled(if elegida { "▍" } else { " " }, fg(p.accent)),
                Span::styled(icono, fg(color)),
                Span::styled(pad(&truncate(&e.nombre, ancho), ancho), estilo),
            ],
        );
        app.hits
            .push((Rect::new(r.x, y, r.width, 1), Hit::Entrada(i)));
    }
}

fn miniatura(app: &mut App, f: &mut Frame, r: Rect, ex: &Explorador) {
    let p = app.paleta.clone();
    let Some(e) = ex.actual().filter(|e| e.tipo == Tipo::Imagen) else {
        put(
            f,
            r.x,
            r.y,
            vec![Span::styled(t("archivos.elige"), fg(p.muted))],
        );
        return;
    };
    app.imagenes.request(&e.ruta);
    match app.imagenes.entry(&e.ruta) {
        Some(Entry::Ready(proto)) => {
            f.render_stateful_widget(
                StatefulImage::default().resize(Resize::Fit(None)),
                r,
                &mut **proto,
            );
        }
        Some(Entry::Failed) => {
            put(
                f,
                r.x,
                r.y,
                vec![Span::styled(t("archivos.no_se_lee"), fg(p.warn))],
            );
        }
        _ => {
            put(
                f,
                r.x,
                r.y,
                vec![Span::styled(t("archivos.cargando"), fg(p.muted))],
            );
        }
    }
}

fn color(
    app: &mut App,
    f: &mut Frame,
    area: Rect,
    campo: Campo,
    sel: usize,
    escrito: Option<&str>,
) {
    let opciones = app.vista.sugerencias.len() + 2;
    let titulo = t(if campo == Campo::Fondo {
        "dialogo.fondo"
    } else {
        "dialogo.texto"
    });
    let r = marco(app, f, centered(area, 48, opciones as u16 + 9), &titulo);
    let p = app.paleta.clone();
    let resuelto = app.vista.colores.map(|c| {
        if campo == Campo::Fondo {
            c.fondo
        } else {
            c.texto
        }
    });
    for i in 0..opciones {
        let y = r.y + i as u16 + if i > 0 { 2 } else { 0 };
        let elegida = i == sel;
        let fuerte = elegida || app.hover == Some(Hit::OpcionColor(i));
        let estilo = if fuerte {
            fg(p.bright).add_modifier(Modifier::BOLD)
        } else {
            fg(p.fg)
        };
        let mut spans = vec![Span::styled(if elegida { "▍" } else { " " }, fg(p.accent))];
        if i == 0 {
            spans.extend(muestra_de_color(&p, Tono::Automatico, resuelto, estilo));
        } else if let Some(c) = app.vista.sugerencias.get(i - 1) {
            spans.push(Span::styled(
                "██ ",
                Style::new().fg(Color::Rgb(c.0, c.1, c.2)),
            ));
            spans.push(Span::styled(to_hex(*c), estilo));
        } else {
            spans.push(Span::styled(t("color.escribir"), estilo));
        }
        put(f, r.x, y, spans);
        app.hits
            .push((Rect::new(r.x, y, r.width, 1), Hit::OpcionColor(i)));
    }
    if !app.vista.sugerencias.is_empty() {
        seccion(&p, f, r.x + 1, r.y + 1, &t("color.del_dibujo"));
    }
    if let Some(texto) = escrito {
        let y = r.y + opciones as u16 + 2;
        put(
            f,
            r.x + 1,
            y,
            vec![
                Span::styled(texto.to_string(), fg(p.bright)),
                Span::styled("▏", fg(p.accent)),
            ],
        );
    }
    ayuda(
        app,
        f,
        r.x,
        r.bottom() - 1,
        &[("⏎", t("atajo.elegir")), ("Esc", t("atajo.cancelar"))],
    );
}

fn borrar(app: &App, f: &mut Frame, area: Rect) {
    let r = marco(app, f, centered(area, 60, 9), &t("dialogo.borrar"));
    let p = &app.paleta;
    let pregunta = tf("borrar.pregunta", &[("nombre", &app.editando.nombre)]);
    put(
        f,
        r.x,
        r.y,
        vec![Span::styled(
            truncate(&pregunta, r.width as usize),
            fg(p.bright).add_modifier(Modifier::BOLD),
        )],
    );
    for (i, linea) in lizarbe_core::ui::wrap(&t("borrar.detalle"), r.width as usize)
        .iter()
        .take(2)
        .enumerate()
    {
        put(
            f,
            r.x,
            r.y + 2 + i as u16,
            vec![Span::styled(linea.clone(), fg(p.muted))],
        );
    }
    ayuda(
        app,
        f,
        r.x,
        r.bottom() - 1,
        &[("⏎", t("atajo.borrar")), ("Esc", t("atajo.cancelar"))],
    );
}
