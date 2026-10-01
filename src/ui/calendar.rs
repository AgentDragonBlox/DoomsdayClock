//! A month-view date picker, drawn in the active skin: the Windows 7
//! calendar (blue gradient header, glassy day highlight) or the Windows 11
//! CalendarView (accent-filled circle for the selected day).

use chrono::{Datelike, Months, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2,
};

use super::paint::{Icon, alpha, gradient_box, hex, mix};
use super::theme::Theme;
use super::widgets;

#[derive(Default)]
pub struct Calendar {
    pub open: bool,
    /// First day of the month being viewed.
    month: Option<NaiveDate>,
    anchor: Pos2,
}

const CELL: f32 = 34.0;

impl Calendar {
    pub fn open_at(&mut self, anchor: Pos2, current: NaiveDate) {
        self.open = true;
        self.anchor = anchor;
        self.month = current.with_day(1);
    }

    /// Shows the popup if open. Returns the picked date, if any.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        th: &Theme,
        selected: NaiveDate,
        today: NaiveDate,
    ) -> Option<NaiveDate> {
        if !self.open {
            return None;
        }
        let month = *self.month.get_or_insert_with(|| selected.with_day(1).unwrap_or(selected));
        let mut picked = None;
        let size = vec2(CELL * 7.0 + 20.0, CELL * 7.0 + 58.0);

        let area = egui::Area::new(egui::Id::new("ddc-calendar")).order(Order::Foreground).fixed_pos(self.anchor).show(
            ctx,
            |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::click());
                let p = ui.painter();
                // Popup body + shadow.
                p.add(
                    egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: alpha(Color32::BLACK, if th.dark { 0.5 } else { 0.22 }),
                    }
                    .as_shape(rect, CornerRadius::same(th.card_radius() as u8)),
                );
                if th.aero() {
                    gradient_box(p, rect, 4.0, &[(0.0, hex(0xFFFFFF)), (1.0, hex(0xF4F7FB))], hex(0x8BA6C7));
                } else {
                    let fill = if th.dark { hex(0x2C2C2C) } else { hex(0xF9F9F9) };
                    p.rect_filled(rect, CornerRadius::same(8), fill);
                    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, th.c.card_border), StrokeKind::Inside);
                }

                // Header: < Month Year >
                let header = Rect::from_min_size(rect.min + vec2(10.0, 8.0), vec2(rect.width() - 20.0, 30.0));
                if th.aero() {
                    gradient_box(
                        p,
                        header,
                        3.0,
                        &[(0.0, hex(0xF2F7FD)), (0.5, hex(0xDDE9F7)), (0.5, hex(0xCEDFF3)), (1.0, hex(0xDCE8F6))],
                        hex(0xA5BEDB),
                    );
                }
                p.text(
                    header.center(),
                    Align2::CENTER_CENTER,
                    month.format("%B %Y").to_string(),
                    th.strong(),
                    if th.aero() { hex(0x1E3287) } else { th.c.text },
                );
                let nav = |ui: &mut egui::Ui, icon: Icon, x: f32| {
                    let r = Rect::from_center_size(
                        pos2(x, header.center().y),
                        Vec2::splat(if th.aero() { 22.0 } else { 28.0 }),
                    );
                    ui.put(r, |ui: &mut egui::Ui| {
                        widgets::icon_button(
                            ui,
                            th,
                            icon,
                            if icon == Icon::ChevronLeft { "Previous month" } else { "Next month" },
                        )
                    })
                    .clicked()
                };
                if nav(ui, Icon::ChevronLeft, header.left() + 16.0) {
                    self.month = month.checked_sub_months(Months::new(1));
                }
                if nav(ui, Icon::ChevronRight, header.right() - 16.0) {
                    self.month = month.checked_add_months(Months::new(1));
                }

                // Weekday row, Monday first.
                let grid_top = header.bottom() + 6.0;
                let p = ui.painter();
                for (i, d) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
                    let c = pos2(rect.left() + 10.0 + CELL * (i as f32 + 0.5), grid_top + 10.0);
                    p.text(c, Align2::CENTER_CENTER, *d, th.small(), th.c.text_dim);
                }

                // Day grid.
                let lead = month.weekday().num_days_from_monday() as i64;
                let first_cell = month - chrono::Duration::days(lead);
                for i in 0..42 {
                    let day = first_cell + chrono::Duration::days(i);
                    let (row, col) = (i / 7, i % 7);
                    let cell = Rect::from_min_size(
                        pos2(rect.left() + 10.0 + CELL * col as f32, grid_top + 22.0 + CELL * row as f32),
                        Vec2::splat(CELL),
                    );
                    let in_month = day.month() == month.month();
                    let selectable = day > today;
                    let resp = ui.interact(
                        cell,
                        egui::Id::new(("cal", i)),
                        if selectable { Sense::click() } else { Sense::hover() },
                    );
                    let p = ui.painter();
                    let is_sel = day == selected;
                    let inner = cell.shrink(2.0);
                    if th.aero() {
                        if is_sel {
                            gradient_box(p, inner, 3.0, &[(0.0, hex(0xDCEBFC)), (1.0, hex(0xC1DBFC))], hex(0x7DA2CE));
                        } else if resp.hovered() && selectable {
                            gradient_box(p, inner, 3.0, &[(0.0, hex(0xFAFBFD)), (1.0, hex(0xEBF3FD))], hex(0xB8D6FB));
                        }
                        if day == today {
                            p.rect_stroke(
                                inner,
                                CornerRadius::same(3),
                                Stroke::new(1.0, hex(0xD1840C)),
                                StrokeKind::Inside,
                            );
                        }
                    } else {
                        if is_sel {
                            p.circle_filled(cell.center(), CELL / 2.0 - 3.0, th.accent);
                        } else if resp.hovered() && selectable {
                            p.circle_filled(cell.center(), CELL / 2.0 - 3.0, th.c.hover);
                        }
                        if day == today && !is_sel {
                            p.circle_stroke(cell.center(), CELL / 2.0 - 3.0, Stroke::new(1.0, th.accent));
                        }
                    }
                    let col = if is_sel && !th.aero() {
                        th.c.on_accent
                    } else if !selectable {
                        alpha(th.c.text_dim, 0.45)
                    } else if in_month {
                        th.c.text
                    } else {
                        mix(th.c.text_dim, Color32::TRANSPARENT, 0.3)
                    };
                    p.text(cell.center(), Align2::CENTER_CENTER, day.day().to_string(), th.body(), col);
                    if resp.clicked() {
                        picked = Some(day);
                    }
                }
                let hint = Rect::from_min_size(pos2(rect.left(), rect.bottom() - 22.0), vec2(rect.width(), 18.0));
                ui.painter().text(
                    hint.center(),
                    Align2::CENTER_CENTER,
                    "Pick any day after today",
                    th.small(),
                    th.c.text_dim,
                );
            },
        );

        let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|pos| !area.response.rect.contains(pos));
        if picked.is_some() || clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.open = false;
        }
        picked
    }
}
