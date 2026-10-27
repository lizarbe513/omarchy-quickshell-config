//! Teclado y ratón: traduce cada pulsación o clic en una acción de `app`,
//! según el diálogo abierto o el panel con el foco. También engancha `App`
//! al bucle de `lizarbe_core::term`.

use std::time::Duration;

use lizarbe_core::mouse::{self, Mouse};
use lizarbe_core::term::{Command, TuiApp};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use crate::app::{App, Campo, Dialogo, Foco, Hit};
use crate::archivos;
use crate::paleta::Tono;
use crate::ui;

impl App {
    fn tecla(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.pedir_salir();
            return;
        }
        match self.dialogo.take() {
            Some(d) => self.tecla_dialogo(d, key.code),
            None => self.tecla_principal(key.code),
        }
    }

    fn tecla_principal(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.pedir_salir(),
            KeyCode::Esc if self.foco == Foco::Editor => self.foco = Foco::Lista,
            KeyCode::Esc => self.pedir_salir(),
            KeyCode::Tab | KeyCode::BackTab => {
                self.foco = if self.foco == Foco::Lista {
                    Foco::Editor
                } else {
                    Foco::Lista
                };
            }
            KeyCode::Char('s') => {
                self.guardar();
            }
            KeyCode::Char('a') => self.aplicar(),
            KeyCode::Char('n') => self.nueva(),
            KeyCode::Char('d') => self.duplicar(),
            KeyCode::Char('x') | KeyCode::Delete => self.pedir_borrar(),
            _ if self.foco == Foco::Lista => self.tecla_lista(code),
            _ => self.tecla_editor(code),
        }
    }

    fn tecla_lista(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.mover(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover(1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.foco = Foco::Editor,
            _ => {}
        }
    }

    fn tecla_editor(&mut self, code: KeyCode) {
        let i = Campo::TODOS
            .iter()
            .position(|c| *c == self.campo)
            .unwrap_or(0);
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.campo = Campo::TODOS[i.saturating_sub(1)],
            KeyCode::Down | KeyCode::Char('j') => {
                self.campo = Campo::TODOS[(i + 1).min(Campo::TODOS.len() - 1)];
            }
            KeyCode::Left | KeyCode::Char('h' | '-') if self.campo == Campo::Altura => {
                self.cambiar_altura(false);
            }
            KeyCode::Right | KeyCode::Char('l' | '+') if self.campo == Campo::Altura => {
                self.cambiar_altura(true);
            }
            KeyCode::Left | KeyCode::Char('h') => self.foco = Foco::Lista,
            KeyCode::Enter | KeyCode::Char(' ') => self.editar_campo(self.campo),
            _ => {}
        }
    }

    /// Teclas con un diálogo abierto. El diálogo se devuelve a su sitio salvo
    /// que la tecla lo cierre.
    fn tecla_dialogo(&mut self, d: Dialogo, code: KeyCode) {
        self.dialogo = match d {
            Dialogo::Nombre(texto) => self.tecla_nombre(texto, code),
            Dialogo::Explorador(ex) => self.tecla_explorador(ex, code),
            Dialogo::Color {
                campo,
                sel,
                escrito,
            } => self.tecla_color(campo, sel, escrito, code),
            Dialogo::Borrar => match code {
                KeyCode::Enter | KeyCode::Char('s' | 'y') => {
                    self.borrar();
                    None
                }
                KeyCode::Esc | KeyCode::Char('n' | 'q') => None,
                _ => Some(Dialogo::Borrar),
            },
        };
    }

    fn tecla_nombre(&mut self, mut texto: String, code: KeyCode) -> Option<Dialogo> {
        match code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                self.poner_nombre(&texto);
                return None;
            }
            KeyCode::Backspace => {
                texto.pop();
            }
            KeyCode::Char(c) => texto.push(c),
            _ => {}
        }
        Some(Dialogo::Nombre(texto))
    }

    fn tecla_explorador(&mut self, mut ex: archivos::Explorador, code: KeyCode) -> Option<Dialogo> {
        if ex.escrita.is_some() {
            return self.tecla_ruta_escrita(ex, code);
        }
        match code {
            KeyCode::Char(c @ ('/' | '~')) => ex.escrita = Some(c.to_string()),
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Up | KeyCode::Char('k') => ex.mover(-1),
            KeyCode::Down | KeyCode::Char('j') => ex.mover(1),
            KeyCode::PageUp => ex.mover(-10),
            KeyCode::PageDown => ex.mover(10),
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => ex.subir(),
            KeyCode::Char(c @ '1'..='9') => {
                let n = c as usize - '1' as usize;
                if let Some(a) = archivos::atajos().get(n) {
                    ex.ir_a(&a.carpeta);
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(ruta) = ex.activar() {
                    self.poner_dibujo(ruta);
                    return None;
                }
            }
            _ => {}
        }
        Some(Dialogo::Explorador(ex))
    }

    /// Escribir una ruta en el explorador: ⏎ va a ella (o la elige si es una imagen).
    fn tecla_ruta_escrita(
        &mut self,
        mut ex: archivos::Explorador,
        code: KeyCode,
    ) -> Option<Dialogo> {
        let texto = ex.escrita.get_or_insert_with(String::new);
        match code {
            KeyCode::Esc => ex.escrita = None,
            KeyCode::Backspace => {
                texto.pop();
            }
            KeyCode::Char(c) => texto.push(c),
            KeyCode::Enter => match ex.ir_a_escrita() {
                Ok(Some(ruta)) => {
                    self.poner_dibujo(ruta);
                    return None;
                }
                Ok(None) => {}
                Err(()) => self.avisar(crate::i18n::t("aviso.ruta_invalida"), false),
            },
            _ => {}
        }
        Some(Dialogo::Explorador(ex))
    }

    fn tecla_color(
        &mut self,
        campo: Campo,
        sel: usize,
        escrito: Option<String>,
        code: KeyCode,
    ) -> Option<Dialogo> {
        if let Some(texto) = escrito {
            return self.tecla_color_escrito(campo, sel, texto, code);
        }
        let opciones = self.vista.sugerencias.len() + 2;
        let sel = match code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Up | KeyCode::Char('k') => sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (sel + 1).min(opciones - 1),
            KeyCode::Enter | KeyCode::Char(' ') => return self.elegir_color(campo, sel),
            _ => sel,
        };
        Some(Dialogo::Color {
            campo,
            sel,
            escrito: None,
        })
    }

    /// Opción `sel` del selector: 0 automático, después los colores del
    /// dibujo y al final «escribir código».
    pub fn elegir_color(&mut self, campo: Campo, sel: usize) -> Option<Dialogo> {
        let sugerencias = self.vista.sugerencias.clone();
        if sel == 0 {
            self.poner_tono(campo, Tono::Automatico);
        } else if let Some(c) = sugerencias.get(sel - 1) {
            self.poner_tono(campo, Tono::Fijo(*c));
        } else {
            let actual = match self.tono(campo) {
                Tono::Fijo(_) => self.tono(campo).a_texto(),
                Tono::Automatico => "#".into(),
            };
            return Some(Dialogo::Color {
                campo,
                sel,
                escrito: Some(actual),
            });
        }
        None
    }

    fn tecla_color_escrito(
        &mut self,
        campo: Campo,
        sel: usize,
        mut texto: String,
        code: KeyCode,
    ) -> Option<Dialogo> {
        match code {
            KeyCode::Esc => {
                return Some(Dialogo::Color {
                    campo,
                    sel,
                    escrito: None,
                });
            }
            KeyCode::Enter => match Tono::desde_texto(&texto) {
                Tono::Fijo(c) => {
                    self.poner_tono(campo, Tono::Fijo(c));
                    return None;
                }
                Tono::Automatico => self.avisar(crate::i18n::t("aviso.color_invalido"), false),
            },
            KeyCode::Backspace => {
                texto.pop();
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() || c == '#' => texto.push(c),
            _ => {}
        }
        Some(Dialogo::Color {
            campo,
            sel,
            escrito: Some(texto),
        })
    }

    fn raton(&mut self, m: MouseEvent) {
        match mouse::read(m, &self.hits, &mut self.clicks) {
            Mouse::Hover(h) => self.hover = h,
            Mouse::Scroll { down, .. } => self.desplazar(if down { 1 } else { -1 }),
            Mouse::Click {
                hit: Some(h),
                double,
                ..
            } => self.clic(h, double),
            Mouse::Click { hit: None, .. } if matches!(self.dialogo, Some(Dialogo::Borrar)) => {
                self.dialogo = None;
            }
            _ => {}
        }
    }

    fn desplazar(&mut self, delta: isize) {
        match &mut self.dialogo {
            Some(Dialogo::Explorador(ex)) => ex.mover(delta),
            Some(_) => {}
            None if self.foco == Foco::Lista => self.mover(delta),
            None => {
                if self.campo == Campo::Altura {
                    self.cambiar_altura(delta < 0);
                }
            }
        }
    }

    fn clic(&mut self, hit: Hit, doble: bool) {
        match (hit, self.dialogo.take()) {
            (Hit::Entrada(i), Some(Dialogo::Explorador(mut ex))) => {
                ex.sel = i;
                let elegida = if doble { ex.activar() } else { None };
                match elegida {
                    Some(ruta) => self.poner_dibujo(ruta),
                    None => self.dialogo = Some(Dialogo::Explorador(ex)),
                }
            }
            (Hit::Atajo(i), Some(Dialogo::Explorador(mut ex))) => {
                if let Some(a) = archivos::atajos().get(i) {
                    ex.ir_a(&a.carpeta);
                }
                self.dialogo = Some(Dialogo::Explorador(ex));
            }
            (Hit::OpcionColor(i), Some(Dialogo::Color { campo, .. })) => {
                self.dialogo = self.elegir_color(campo, i);
            }
            (_, Some(d)) => self.dialogo = Some(d),
            (Hit::Pantalla(i), None) => {
                self.foco = Foco::Lista;
                self.seleccionar(i);
            }
            (Hit::Campo(c), None) => self.editar_campo(c),
            _ => {}
        }
    }
}

impl TuiApp for App {
    fn draw(&mut self, f: &mut Frame) {
        ui::dibujar(self, f);
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.tecla(key);
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        self.raton(m);
    }

    fn tick(&mut self) {
        App::tick(self);
    }

    fn take_command(&mut self) -> Option<Command> {
        self.tomar_comando()
    }

    fn after_command(&mut self, result: Result<bool, String>) {
        self.tras_comando(result);
    }

    fn should_quit(&self) -> bool {
        self.salir_pedido()
    }

    fn esc_cancels(&self) -> bool {
        true
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_millis(100)
    }
}
