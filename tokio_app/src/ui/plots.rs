use crate::data::filter::sharping_decimate;
use crate::data::processor::DataProcessor;
use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke};
use egui_plot::{Axis, AxisHints, GridMark, Line, Plot, PlotBounds, PlotPoint, PlotPoints, PlotTransform};
use std::collections::HashMap;
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeScale {
    Scale2s,
    Scale5s,
    Scale10s,
    Scale30s,
}

impl TimeScale {
    pub fn get_seconds(&self) -> f64 {
        match self {
            TimeScale::Scale2s => 2.0,
            TimeScale::Scale5s => 5.0,
            TimeScale::Scale10s => 10.0,
            TimeScale::Scale30s => 30.0,
        }
    }

    pub fn get_name(&self) -> &'static str {
        match self {
            TimeScale::Scale2s => "2s",
            TimeScale::Scale5s => "5s",
            TimeScale::Scale10s => "10s",
            TimeScale::Scale30s => "30s",
        }
    }

    pub fn next_scale(&self) -> TimeScale {
        match self {
            TimeScale::Scale2s => TimeScale::Scale5s,
            TimeScale::Scale5s => TimeScale::Scale10s,
            TimeScale::Scale10s => TimeScale::Scale30s,
            TimeScale::Scale30s => TimeScale::Scale2s,
        }
    }

    pub fn prev_scale(&self) -> TimeScale {
        match self {
            TimeScale::Scale2s => TimeScale::Scale30s,
            TimeScale::Scale5s => TimeScale::Scale2s,
            TimeScale::Scale10s => TimeScale::Scale5s,
            TimeScale::Scale30s => TimeScale::Scale10s,
        }
    }
}

const MIN_VIEW_SPAN_S: f64 = 0.05;
/// Минимальный размер выделения в пикселях (иначе считаем кликом).
const MIN_BOX_PX: f32 = 8.0;

#[derive(Debug, Clone, Copy)]
struct YBounds {
    min: f64,
    max: f64,
}

impl YBounds {
    fn from_points(points: &[(f64, f64)], force_zero: bool) -> Option<Self> {
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        for (_, v) in points {
            min_v = min_v.min(*v);
            max_v = max_v.max(*v);
        }
        if !min_v.is_finite() || !max_v.is_finite() {
            return None;
        }
        if force_zero {
            min_v = min_v.min(0.0);
            max_v = max_v.max(0.0);
        }
        let mut span = (max_v - min_v).abs();
        if span < 1e-9 {
            span = (max_v.abs() + 1.0).max(1.0) * 0.1;
        }
        let pad = span * 0.15;
        Some(Self {
            min: min_v - pad,
            max: max_v + pad,
        })
    }

    fn follow(&mut self, other: Self) {
        const ALPHA: f64 = 0.25;
        self.min += (other.min - self.min) * ALPHA;
        self.max += (other.max - self.max) * ALPHA;
        if other.min < self.min {
            self.min = other.min;
        }
        if other.max > self.max {
            self.max = other.max;
        }
        if self.max - self.min < 1e-6 {
            self.max = self.min + 1.0;
        }
    }
}

#[derive(Debug, Clone)]
struct BoxZoomDrag {
    /// Канал, на котором тянут рамку (Y зумится только у него).
    channel: String,
    start_screen: Pos2,
    start_plot: PlotPoint,
}

pub struct PlotManager {
    time_scale: TimeScale,
    /// Правый край окна (live) / конец диапазона (offline).
    current_view_end: f64,
    /// Левый край окна в offline-режиме.
    view_start: f64,
    /// Просмотр .bin: свободная навигация по оси X.
    offline: bool,
    /// Длительность загруженных данных (с).
    data_end: f64,
    y_bounds: HashMap<String, YBounds>,
    /// Y, зафиксированный выделением (не следует за данными).
    y_manual: HashMap<String, YBounds>,
    box_drag: Option<BoxZoomDrag>,
}

impl PlotManager {
    pub fn new() -> Self {
        Self {
            time_scale: TimeScale::Scale5s,
            current_view_end: 5.0,
            view_start: 0.0,
            offline: false,
            data_end: 0.0,
            y_bounds: HashMap::new(),
            y_manual: HashMap::new(),
            box_drag: None,
        }
    }

    pub fn reset_y_bounds(&mut self) {
        self.y_bounds.clear();
        self.y_manual.clear();
    }

    /// Режим просмотра `.bin`: весь файл, зум выделением.
    pub fn enter_offline_view(&mut self, duration_s: f64) {
        let end = duration_s.max(MIN_VIEW_SPAN_S);
        self.offline = true;
        self.data_end = end;
        self.view_start = 0.0;
        self.current_view_end = end;
        self.box_drag = None;
        self.reset_y_bounds();
    }

    /// Вернуться к live sticky-окну.
    pub fn enter_live_view(&mut self) {
        self.offline = false;
        self.box_drag = None;
        self.y_manual.clear();
        self.data_end = 0.0;
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn fit_offline_view(&mut self) {
        if !self.offline {
            return;
        }
        self.view_start = 0.0;
        self.current_view_end = self.data_end.max(MIN_VIEW_SPAN_S);
        self.box_drag = None;
        self.reset_y_bounds();
    }

    pub fn show_plots(&mut self, ui: &mut egui::Ui, data_processor: &DataProcessor) {
        if self.offline {
            self.data_end = data_processor.get_max_time().max(self.data_end);
            self.clamp_offline_view();
        } else {
            self.update_view_range(data_processor.get_max_time());
        }

        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.offline {
                        ui.label("Окно:");
                        ui.label(format!(
                            "{:.2}s – {:.2}s  (Δ {:.2}s)",
                            self.view_start,
                            self.current_view_end,
                            self.view_span()
                        ));
                        if ui.button("Fit").clicked() {
                            self.fit_offline_view();
                        }
                        if ui.button("−").clicked() {
                            self.zoom_out();
                        }
                        if ui.button("+").clicked() {
                            self.zoom_in();
                        }
                        if ui.button("Y reset").clicked() {
                            self.reset_y_bounds();
                        }
                        ui.label("ЛКМ: зум по времени · двойной клик: Fit");
                    } else {
                        ui.label("Time scale:");
                        ui.label(format!("{}", self.time_scale.get_name()));
                        if ui.button("−").clicked() {
                            self.zoom_out();
                        }
                        if ui.button("+").clicked() {
                            self.zoom_in();
                        }
                        if ui.button("Y reset").clicked() {
                            self.reset_y_bounds();
                        }
                        let view_start = self.current_view_end - self.time_scale.get_seconds();
                        ui.label(format!(
                            "View: {:.1}s - {:.1}s",
                            view_start, self.current_view_end
                        ));
                    }
                });

                ui.separator();
                ui.heading("ЭКГ");
                self.show_channel_plot(ui, data_processor.get_ecg(), "ЭКГ", true);

                ui.separator();
                ui.heading("РЕО-1");
                self.show_channel_plot(ui, data_processor.get_rheocardiogram(), "РЕО-1", true);

                ui.separator();
                ui.heading("BASE-1");
                self.show_channel_plot(ui, data_processor.get_base_impedance(), "BASE-1", false);

                ui.separator();
                ui.heading("РЕО-2");
                self.show_channel_plot(ui, data_processor.get_rheo2(), "РЕО-2", true);

                ui.separator();
                ui.heading("BASE-2");
                self.show_channel_plot(ui, data_processor.get_base2(), "BASE-2", false);
            });
    }

    fn view_span(&self) -> f64 {
        (self.current_view_end - self.view_start).max(MIN_VIEW_SPAN_S)
    }

    fn view_range(&self) -> (f64, f64) {
        if self.offline {
            (self.view_start, self.current_view_end)
        } else {
            let end = self.current_view_end;
            (end - self.time_scale.get_seconds(), end)
        }
    }

    fn show_channel_plot(
        &mut self,
        ui: &mut egui::Ui,
        data: &[(f64, f64)],
        name: &str,
        force_zero: bool,
    ) {
        if data.is_empty() {
            ui.label("No data available");
            return;
        }

        let (view_start, view_end) = self.view_range();
        let window = window_slice(data, view_start, view_end);
        if window.is_empty() {
            ui.label("No data in current view range");
            return;
        }

        // Целимся в реальную ширину графика; при достаточном числе точек
        // не прореживаем — иначе min/max-огибающая превращается в «пилы».
        let width_px = ui.available_width().max(64.0) as f64;
        let seconds = (view_end - view_start).max(0.1);
        let points_in_sec = width_px / seconds;
        let mut factor = ((200.0 / points_in_sec).round() as usize).max(1);
        if window.len() as f64 <= width_px * 2.0 {
            factor = 1;
        }
        let points = sharping_decimate(window, factor);

        // Y считаем по полному окну (не по прореженному), чтобы пики не «срезались».
        let y = if let Some(manual) = self.y_manual.get(name).copied() {
            manual
        } else if let Some(fresh) = YBounds::from_points(window, force_zero) {
            if self.offline {
                // В .bin сразу подгоняем шкалу под видимый фрагмент.
                self.y_bounds.insert(name.to_string(), fresh);
                fresh
            } else {
                self.y_bounds
                    .entry(name.to_string())
                    .and_modify(|b| b.follow(fresh))
                    .or_insert(fresh);
                *self.y_bounds.get(name).unwrap()
            }
        } else {
            self.y_bounds
                .get(name)
                .copied()
                .unwrap_or(YBounds {
                    min: -1.0,
                    max: 1.0,
                })
        };

        let plot_points: PlotPoints = points.iter().map(|(t, v)| [*t, *v]).collect();
        let line = Line::new(plot_points)
            .name(name)
            .color(Color32::BLACK)
            .width(1.25);
        let bounds = PlotBounds::from_min_max([view_start, y.min], [view_end, y.max]);

        let plot_response = Plot::new(name)
            .height(180.0)
            .width(ui.available_width())
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .auto_bounds([false, false].into())
            .sense(Sense::click_and_drag())
            .custom_x_axes(vec![AxisHints::new(Axis::X).label("t, с")])
            .x_axis_formatter(format_time_axis)
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds(bounds);
                plot_ui.line(line);
            });

        if self.offline {
            self.handle_offline_interaction(ui, name, &plot_response.response, plot_response.transform);
        }
    }

    fn handle_offline_interaction(
        &mut self,
        ui: &mut egui::Ui,
        channel: &str,
        response: &egui::Response,
        transform: PlotTransform,
    ) {
        if response.double_clicked() {
            self.fit_offline_view();
            return;
        }

        // ЛКМ: резиновая рамка зума.
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let start_plot = transform.value_from_position(pos);
                self.box_drag = Some(BoxZoomDrag {
                    channel: channel.to_string(),
                    start_screen: pos,
                    start_plot,
                });
            }
        }

        let dragging_this = self
            .box_drag
            .as_ref()
            .is_some_and(|d| d.channel == channel);

        if dragging_this {
            if let Some(drag) = self.box_drag.clone() {
                if let Some(cur) = response.interact_pointer_pos().or_else(|| response.hover_pos()) {
                    let rect = Rect::from_two_pos(drag.start_screen, cur);
                    let painter = ui.painter().with_clip_rect(response.rect);
                    painter.rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(2.0, Color32::from_rgb(30, 90, 200)),
                    );
                    painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(30, 90, 200, 40));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
                }
            }
        }

        if response.drag_stopped() {
            let Some(drag) = self.box_drag.take() else {
                return;
            };
            if drag.channel != channel {
                return;
            }
            let Some(end_screen) = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|i| i.pointer.latest_pos()))
            else {
                return;
            };

            if (end_screen - drag.start_screen).length() < MIN_BOX_PX {
                return;
            }

            let end_plot = transform.value_from_position(end_screen);
            let x0 = drag.start_plot.x.min(end_plot.x);
            let x1 = drag.start_plot.x.max(end_plot.x);
            // Только ось X: Y рамки часто обрезает пики («урезанные пилы»).
            self.apply_box_zoom_x(x0, x1);
        }

        // СКМ / ПКМ: панорамирование по X.
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = response.drag_delta();
            if delta.x.abs() > 0.0 {
                let dx_data = -delta.x as f64 * transform.dvalue_dpos()[0];
                self.pan_offline(dx_data);
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    fn apply_box_zoom_x(&mut self, x0: f64, x1: f64) {
        let mut x_min = x0.min(x1);
        let mut x_max = x0.max(x1);
        if x_max - x_min < MIN_VIEW_SPAN_S {
            let mid = 0.5 * (x_min + x_max);
            x_min = mid - MIN_VIEW_SPAN_S * 0.5;
            x_max = mid + MIN_VIEW_SPAN_S * 0.5;
        }
        x_min = x_min.clamp(0.0, self.data_end);
        x_max = x_max.clamp(0.0, self.data_end);
        if x_max - x_min < MIN_VIEW_SPAN_S {
            x_max = (x_min + MIN_VIEW_SPAN_S).min(self.data_end);
            x_min = (x_max - MIN_VIEW_SPAN_S).max(0.0);
        }
        self.view_start = x_min;
        self.current_view_end = x_max;
        // Пересчитать Y по новому окну (без обрезки рамкой).
        self.y_manual.clear();
        self.y_bounds.clear();
    }

    fn pan_offline(&mut self, dx: f64) {
        let span = self.view_span();
        let mut start = self.view_start + dx;
        let mut end = self.current_view_end + dx;
        if start < 0.0 {
            start = 0.0;
            end = span;
        }
        if end > self.data_end {
            end = self.data_end;
            start = (end - span).max(0.0);
        }
        self.view_start = start;
        self.current_view_end = end;
    }

    fn clamp_offline_view(&mut self) {
        if self.data_end < MIN_VIEW_SPAN_S {
            self.view_start = 0.0;
            self.current_view_end = MIN_VIEW_SPAN_S;
            return;
        }
        if self.current_view_end > self.data_end {
            let span = self.view_span();
            self.current_view_end = self.data_end;
            self.view_start = (self.current_view_end - span).max(0.0);
        }
        if self.view_start < 0.0 {
            self.view_start = 0.0;
        }
        if self.current_view_end <= self.view_start {
            self.current_view_end = (self.view_start + MIN_VIEW_SPAN_S).min(self.data_end.max(MIN_VIEW_SPAN_S));
        }
    }

    fn update_view_range(&mut self, max_time: f64) {
        let scale_seconds = self.time_scale.get_seconds();
        if max_time < scale_seconds {
            self.current_view_end = scale_seconds;
        } else {
            self.current_view_end = max_time;
        }
    }

    pub fn zoom_in(&mut self) {
        if self.offline {
            self.zoom_offline(0.6);
        } else {
            self.time_scale = self.time_scale.prev_scale();
        }
    }

    pub fn zoom_out(&mut self) {
        if self.offline {
            self.zoom_offline(1.0 / 0.6);
        } else {
            self.time_scale = self.time_scale.next_scale();
        }
    }

    fn zoom_offline(&mut self, factor: f64) {
        let span = self.view_span();
        let mid = 0.5 * (self.view_start + self.current_view_end);
        let mut new_span = (span * factor).max(MIN_VIEW_SPAN_S);
        new_span = new_span.min(self.data_end.max(MIN_VIEW_SPAN_S));
        let mut start = mid - new_span * 0.5;
        let mut end = mid + new_span * 0.5;
        if start < 0.0 {
            start = 0.0;
            end = new_span.min(self.data_end);
        }
        if end > self.data_end {
            end = self.data_end;
            start = (end - new_span).max(0.0);
        }
        self.view_start = start;
        self.current_view_end = end;
        self.y_manual.clear();
        self.y_bounds.clear();
    }

    pub fn get_time_scale(&self) -> TimeScale {
        self.time_scale
    }
}

fn window_slice(data: &[(f64, f64)], view_start: f64, view_end: f64) -> &[(f64, f64)] {
    let start_idx = data.partition_point(|(t, _)| *t < view_start);
    let end_idx = data.partition_point(|(t, _)| *t <= view_end);
    if start_idx >= end_idx {
        &[]
    } else {
        &data[start_idx..end_idx]
    }
}

/// Подписи оси X: секунды от начала записи (не отсчёты).
fn format_time_axis(mark: GridMark, _digits: usize, range: &RangeInclusive<f64>) -> String {
    let v = mark.value;
    let span = (range.end() - range.start()).abs();
    if span >= 100.0 {
        format!("{v:.0} с")
    } else if span >= 10.0 {
        format!("{v:.1} с")
    } else if span >= 1.0 {
        format!("{v:.2} с")
    } else {
        format!("{v:.3} с")
    }
}
