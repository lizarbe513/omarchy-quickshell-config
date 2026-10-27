//! Dibujo de la pantalla principal (estilo `diseno-terminal`): cabecera con
//! la pantalla puesta en el arranque, la lista de pantallas a la izquierda,
//! el editor y la vista previa a la derecha y una barra inferior con el
//! estado y los atajos. Los diálogos los pinta `dialogos`.

use lizarbe_core::color::to_hex;
use lizarbe_core::theme::Palette;
use lizarbe_core::ui::{fg, pad, put, rule_h, rule_v, truncate};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui_image::{Resize, StatefulImage};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Campo, Foco, Hit};
use crate::composicion::{ALTURA_MAX, ALTURA_MIN};
use crate::dialogos;
use crate::i18n::{t, tf};
use crate::paleta::{Rgb, Tono};
use crate::vista::Estado;

/// Tamaño mínimo de la ventana.
const MINIMO: (u16, u16) = (80, 24);
/// Ancho de la columna de la lista.
const ANCHO_LISTA: u16 = 28;
/// Ancho de las etiquetas del editor.
const ANCHO_ETIQUETA: usize = 10;
/// Ancho de la pista del deslizador de tamaño.
const ANCHO_PISTA: usize = 24;

pub fn dibujar(app: &mut App, f: &mut Frame) {
    app.hits.clear();
    let area = f.area();
    let p = app.paleta.clone();
    f.render_widget(Block::new().style(Style::new().bg(p.bg).fg(p.fg)), area);
    if area.width < MINIMO.0 || area.height < MINIMO.1 {
        put(
            f,
            area.x + 1,
            area.y,
            vec![Span::styled(
                truncate(&t("peq"), area.width as usize - 1),
                fg(p.muted),
            )],
        );
        return;
    }
    cabecera(app, f, Rect::new(area.x, area.y, area.width, 1));
    rule_h(f, area.x, area.y + 1, area.width, p.rule);
    let cuerpo = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
    lista(
        app,
        f,
        Rect::new(cuerpo.x, cuerpo.y, ANCHO_LISTA, cuerpo.height),
    );
    rule_v(f, cuerpo.x + ANCHO_LISTA, cuerpo.y, cuerpo.height, p.rule);
    let derecha = Rect::new(
        cuerpo.x + ANCHO_LISTA + 2,
        cuerpo.y,
        cuerpo.width - ANCHO_LISTA - 3,
        cuerpo.height,
    );
    editor(app, f, derecha);
    rule_h(f, area.x, area.bottom() - 2, area.width, p.rule);
    pie(app, f, Rect::new(area.x, area.bottom() - 1, area.width, 1));
    dialogos::dibujar(app, f, area);
}

fn cabecera(app: &App, f: &mut Frame, r: Rect) {
    let p = &app.paleta;
    put(
        f,
        r.x + 2,
        r.y,
        vec![Span::styled(
            t("titulo"),
            fg(p.bright).add_modifier(Modifier::BOLD),
        )],
    );
    let activa = match &app.activa {
        _ if app.en_prueba() => t("cabecera.prueba"),
        Some(a) => tf("cabecera.activa", &[("nombre", a)]),
        None => t("cabecera.ninguna"),
    };
    let w = activa.width() as u16;
    put(
        f,
        r.right().saturating_sub(w + 2),
        r.y,
        vec![Span::styled(activa, fg(p.muted))],
    );
}

/// Etiqueta de sección: mayúsculas cortas y atenuadas.
pub fn seccion(p: &Palette, f: &mut Frame, x: u16, y: u16, texto: &str) {
    put(
        f,
        x,
        y,
        vec![Span::styled(
            texto.to_uppercase(),
            fg(p.muted).add_modifier(Modifier::BOLD),
        )],
    );
}

fn lista(app: &mut App, f: &mut Frame, r: Rect) {
    let p = app.paleta.clone();
    seccion(&p, f, r.x + 2, r.y + 1, &t("lista.titulo"));
    let filas = (r.height as usize).saturating_sub(3);
    let total = app.pantallas.len() + 1;
    let desde = app
        .sel
        .saturating_sub(filas.saturating_sub(1))
        .min(total.saturating_sub(filas.max(1)));
    for (n, i) in (desde..total).take(filas).enumerate() {
        let y = r.y + 3 + n as u16;
        let (texto, activa) = match app.pantallas.get(i) {
            Some(pa) => (
                pa.nombre.clone(),
                app.activa.is_some() && app.activa == pa.carpeta,
            ),
            None => (t("lista.nueva"), false),
        };
        let elegida = i == app.sel;
        let estilo = estilo_fila(
            &p,
            elegida && app.foco == Foco::Lista,
            elegida,
            app.hover == Some(Hit::Pantalla(i)),
        );
        let marca = if elegida { "▍" } else { " " };
        let ancho = r.width as usize - 4 - if activa { 2 } else { 0 };
        let mut spans = vec![
            Span::styled(format!(" {marca}"), fg(p.accent)),
            Span::styled(pad(&truncate(&texto, ancho), ancho), estilo),
        ];
        if activa {
            spans.push(Span::styled(" ●", fg(p.ok)));
        }
        put(f, r.x, y, spans);
        app.hits
            .push((Rect::new(r.x, y, r.width, 1), Hit::Pantalla(i)));
    }
}

fn estilo_fila(p: &Palette, foco: bool, elegida: bool, hover: bool) -> Style {
    if foco || hover {
        fg(p.bright).add_modifier(Modifier::BOLD)
    } else if elegida {
        fg(p.bright)
    } else {
        fg(p.fg)
    }
}

fn editor(app: &mut App, f: &mut Frame, r: Rect) {
    let p = app.paleta.clone();
    seccion(&p, f, r.x, r.y + 1, &t("editor.titulo"));
    for (n, campo) in Campo::TODOS.iter().enumerate() {
        let y = r.y + 3 + n as u16;
        let elegido = app.foco == Foco::Editor && app.campo == *campo;
        let hover = app.hover == Some(Hit::Campo(*campo));
        let marca = if elegido { "▍" } else { " " };
        let mut spans = vec![
            Span::styled(marca, fg(p.accent)),
            Span::styled(pad(&t(clave_campo(*campo)), ANCHO_ETIQUETA), fg(p.muted)),
        ];
        spans.extend(valor(
            app,
            *campo,
            elegido || hover,
            r.width as usize - ANCHO_ETIQUETA - 1,
        ));
        put(f, r.x.saturating_sub(1), y, spans);
        app.hits
            .push((Rect::new(r.x, y, r.width, 1), Hit::Campo(*campo)));
    }
    let y = r.y + 3 + Campo::TODOS.len() as u16 + 1;
    seccion(&p, f, r.x, y, &t("vista.titulo"));
    vista_previa(
        app,
        f,
        Rect::new(r.x, y + 2, r.width, r.bottom().saturating_sub(y + 2)),
    );
}

fn clave_campo(c: Campo) -> &'static str {
    match c {
        Campo::Nombre => "campo.nombre",
        Campo::Dibujo => "campo.dibujo",
        Campo::Altura => "campo.altura",
        Campo::Fondo => "campo.fondo",
        Campo::Texto => "campo.texto",
    }
}

fn valor(app: &App, campo: Campo, fuerte: bool, ancho: usize) -> Vec<Span<'static>> {
    let p = &app.paleta;
    let estilo = if fuerte {
        fg(p.bright).add_modifier(Modifier::BOLD)
    } else {
        fg(p.fg)
    };
    let e = &app.editando;
    match campo {
        Campo::Nombre => vec![Span::styled(truncate(&e.nombre, ancho), estilo)],
        Campo::Dibujo => match &e.dibujo {
            Some(d) => {
                let nombre = d
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                vec![Span::styled(truncate(&nombre, ancho), estilo)]
            }
            None => vec![Span::styled(t("campo.dibujo.vacio"), fg(p.warn))],
        },
        Campo::Altura => deslizador(p, e.altura, estilo),
        Campo::Fondo | Campo::Texto => {
            let resuelto = app.vista.colores.map(|c| {
                if campo == Campo::Fondo {
                    c.fondo
                } else {
                    c.texto
                }
            });
            muestra_de_color(p, app.tono(campo), resuelto, estilo)
        }
    }
}

/// `━━━━━━────  40 %`: la parte llena en acento y la pista atenuada.
fn deslizador(p: &Palette, altura: u8, estilo: Style) -> Vec<Span<'static>> {
    let rango = (ALTURA_MAX - ALTURA_MIN) as usize;
    let lleno = (altura - ALTURA_MIN) as usize * ANCHO_PISTA / rango;
    vec![
        Span::styled("━".repeat(lleno.max(1)), fg(p.accent)),
        Span::styled("─".repeat(ANCHO_PISTA - lleno.max(1)), fg(p.rule)),
        Span::styled(format!("  {altura:>2} %"), estilo),
        Span::styled(format!("   {}", t("campo.altura.ayuda")), fg(p.dim)),
    ]
}

/// `██ #1a1b26  automático`.
pub fn muestra_de_color(
    p: &Palette,
    tono: Tono,
    resuelto: Option<Rgb>,
    estilo: Style,
) -> Vec<Span<'static>> {
    let color = match tono {
        Tono::Fijo(c) => Some(c),
        Tono::Automatico => resuelto,
    };
    let mut spans = match color {
        Some(c) => vec![
            Span::styled("██ ", Style::new().fg(Color::Rgb(c.0, c.1, c.2))),
            Span::styled(to_hex(c), estilo),
        ],
        None => vec![Span::styled("·", fg(p.dim))],
    };
    if tono == Tono::Automatico {
        spans.push(Span::styled(format!("  {}", t("color.auto")), fg(p.muted)));
    }
    spans
}

fn vista_previa(app: &mut App, f: &mut Frame, r: Rect) {
    if r.height < 3 {
        return;
    }
    let p = app.paleta.clone();
    let (texto, color) = match &mut app.vista.estado {
        Estado::Lista(proto) => {
            f.render_stateful_widget(
                StatefulImage::default().resize(Resize::Fit(None)),
                r,
                &mut **proto,
            );
            return;
        }
        Estado::Vacia => (t("vista.vacia"), p.muted),
        Estado::Calculando => (t("vista.calculando"), p.muted),
        Estado::Fallo => (t("vista.fallo"), p.warn),
    };
    put(
        f,
        r.x,
        r.y,
        vec![Span::styled(truncate(&texto, r.width as usize), fg(color))],
    );
}

fn pie(app: &App, f: &mut Frame, r: Rect) {
    let p = &app.paleta;
    let (estado, color) = match &app.aviso {
        Some(a) => (a.texto.clone(), if a.bien { p.ok } else { p.warn }),
        None => (estado(app), p.muted),
    };
    let libre = (r.width as usize).saturating_sub(4);
    let ancho_estado = estado.width().min(libre / 2);
    put(
        f,
        r.x + 2,
        r.y,
        vec![Span::styled(truncate(&estado, ancho_estado), fg(color))],
    );
    // Los atajos que quepan a la derecha, en orden de importancia.
    let mut spans = vec![];
    let mut usado = 0;
    for (k, d) in atajos(app) {
        let separador = if spans.is_empty() { "" } else { "    " };
        let w = separador.len() + k.width() + 1 + d.width();
        if usado + w + ancho_estado + 2 > libre {
            break;
        }
        usado += w;
        spans.push(Span::raw(separador));
        spans.push(Span::styled(k, fg(p.accent)));
        spans.push(Span::styled(format!(" {d}"), fg(p.muted)));
    }
    put(f, r.right().saturating_sub(usado as u16 + 2), r.y, spans);
}

fn estado(app: &App) -> String {
    if app.editando.dibujo.is_none() {
        t("estado.sin_dibujo")
    } else if app.cambios() {
        t("estado.sin_guardar")
    } else if app.es_activa() {
        t("estado.activa")
    } else {
        t("estado.guardada")
    }
}

/// Los atajos de la vista principal, de más a menos importante.
fn atajos(app: &App) -> Vec<(String, String)> {
    let mut v = vec![(
        "⏎".to_string(),
        t(if app.foco == Foco::Lista {
            "atajo.abrir"
        } else {
            "atajo.editar"
        }),
    )];
    v.push(("s".into(), t("atajo.guardar")));
    v.push(("a".into(), t("atajo.aplicar")));
    v.push(("n".into(), t("atajo.nueva")));
    v.push(("q".into(), t("atajo.salir")));
    v.push(("Tab".into(), t("atajo.panel")));
    v.push(("d".into(), t("atajo.duplicar")));
    v.push(("x".into(), t("atajo.borrar")));
    v
}
