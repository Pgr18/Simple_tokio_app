use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use crate::data::processor::DataProcessor;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeScale {
    Scale10s,  // 10 секунд на экран
    Scale30s,  // 30 секунд на экран (по умолчанию)
    Scale60s,  // 60 секунд на экран
}

impl TimeScale {
    pub fn get_seconds(&self) -> f64 {
        match self {
            TimeScale::Scale10s => 10.0,
            TimeScale::Scale30s => 30.0,
            TimeScale::Scale60s => 60.0,
        }
    }

    pub fn get_name(&self) -> &'static str {
        match self {
            TimeScale::Scale10s => "10s",
            TimeScale::Scale30s => "30s",
            TimeScale::Scale60s => "60s",
        }
    }

    pub fn next_scale(&self) -> TimeScale {
        match self {
            TimeScale::Scale10s => TimeScale::Scale30s,
            TimeScale::Scale30s => TimeScale::Scale60s,
            TimeScale::Scale60s => TimeScale::Scale10s, // Циклически возвращаемся к 10s
        }
    }

    pub fn prev_scale(&self) -> TimeScale {
        match self {
            TimeScale::Scale10s => TimeScale::Scale60s, // Циклически переходим к 60s
            TimeScale::Scale30s => TimeScale::Scale10s,
            TimeScale::Scale60s => TimeScale::Scale30s,
        }
    }
}

pub struct PlotManager {
    time_scale: TimeScale,
    current_view_end: f64, // Конец текущего отображаемого интервала
}

impl PlotManager {
    pub fn new() -> Self {
        Self {
            time_scale: TimeScale::Scale30s, // По умолчанию 30 секунд
            current_view_end: 30.0, // Начинаем с отображения от 0 до 30 секунд
        }
    }

    pub fn show_plots(&mut self, ui: &mut egui::Ui, data_processor: &DataProcessor) {
        // Автоматически обновляем конец отображаемого интервала при поступлении новых данных
        self.update_view_range(data_processor.get_max_time());

        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .show(ui, |ui| {
                // Панель управления масштабом
                ui.horizontal(|ui| {
                    ui.label("Time scale:");
                    ui.label(format!("{}", self.time_scale.get_name()));
                    if ui.button("−").clicked() {
                        self.zoom_out();
                    }
                    if ui.button("+").clicked() {
                        self.zoom_in();
                    }
                    ui.label("(Use '+'/'-' keys)");
                    
                    // Показываем текущий временной диапазон
                    let view_start = self.current_view_end - self.time_scale.get_seconds();
                    ui.label(format!("View: {:.1}s - {:.1}s", view_start, self.current_view_end));
                });

                ui.separator();

                // Канал A
                ui.heading("Channel A");
                Self::show_channel_plot(ui, data_processor.get_channel_a(), "Channel A", self.current_view_end, self.time_scale);

                ui.separator();

                // Канал B
                ui.heading("Channel B");
                Self::show_channel_plot(ui, data_processor.get_channel_b(), "Channel B", self.current_view_end, self.time_scale);

                ui.separator();

                // Канал C
                ui.heading("Channel C");
                Self::show_channel_plot(ui, data_processor.get_channel_c(), "Channel C", self.current_view_end, self.time_scale);

                ui.separator();

                // Все каналы вместе
                ui.heading("All Channels");
                Self::show_combined_plot(ui, data_processor, self.current_view_end, self.time_scale);
            });
    }

    fn show_channel_plot(ui: &mut egui::Ui, data: &[(f64, f64)], name: &str, view_end: f64, time_scale: TimeScale) {
        if data.is_empty() {
            ui.label("No data available");
            return;
        }

        // Фильтруем данные для отображения только в текущем диапазоне
        let view_start = view_end - time_scale.get_seconds();
        let filtered_data: Vec<(f64, f64)> = data
            .iter()
            .filter(|(time, _)| *time >= view_start && *time <= view_end)
            .cloned()
            .collect();

        if filtered_data.is_empty() {
            ui.label("No data in current view range");
            return;
        }

        let points: PlotPoints = filtered_data
            .iter()
            .map(|(time_seconds, value)| [*time_seconds, *value])
            .collect();

        let line = Line::new(points).name(name);

        Plot::new(name)
            .height(250.0)
            .width(ui.available_width())
            .x_axis_label("Time (seconds)")
            .y_axis_label("Value")
            .include_x(view_start)
            .include_x(view_end)
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    }

    fn show_combined_plot(ui: &mut egui::Ui, data_processor: &DataProcessor, view_end: f64, time_scale: TimeScale) {
        let view_start = view_end - time_scale.get_seconds();

        let plot = Plot::new("combined_plot")
            .height(300.0)
            .width(ui.available_width())
            .x_axis_label("Time (seconds)")
            .y_axis_label("Value")
            .include_x(view_start)
            .include_x(view_end)
            .legend(egui_plot::Legend::default());

        plot.show(ui, |plot_ui| {
            // Канал A - фильтруем данные перед созданием PlotPoints
            let channel_a_filtered: Vec<(f64, f64)> = data_processor.get_channel_a()
                .iter()
                .filter(|(time, _)| *time >= view_start && *time <= view_end)
                .cloned()
                .collect();
            
            if !channel_a_filtered.is_empty() {
                let points_a: PlotPoints = channel_a_filtered
                    .iter()
                    .map(|(time_seconds, value)| [*time_seconds, *value])
                    .collect();
                let line_a = Line::new(points_a).name("Channel A");
                plot_ui.line(line_a);
            }

            // Канал B - фильтруем данные перед созданием PlotPoints
            let channel_b_filtered: Vec<(f64, f64)> = data_processor.get_channel_b()
                .iter()
                .filter(|(time, _)| *time >= view_start && *time <= view_end)
                .cloned()
                .collect();
            
            if !channel_b_filtered.is_empty() {
                let points_b: PlotPoints = channel_b_filtered
                    .iter()
                    .map(|(time_seconds, value)| [*time_seconds, *value])
                    .collect();
                let line_b = Line::new(points_b).name("Channel B");
                plot_ui.line(line_b);
            }

            // Канал C - фильтруем данные перед созданием PlotPoints
            let channel_c_filtered: Vec<(f64, f64)> = data_processor.get_channel_c()
                .iter()
                .filter(|(time, _)| *time >= view_start && *time <= view_end)
                .cloned()
                .collect();
            
            if !channel_c_filtered.is_empty() {
                let points_c: PlotPoints = channel_c_filtered
                    .iter()
                    .map(|(time_seconds, value)| [*time_seconds, *value])
                    .collect();
                let line_c = Line::new(points_c).name("Channel C");
                plot_ui.line(line_c);
            }
        });
    }

    /// Обновляет диапазон отображения на основе новых данных
    fn update_view_range(&mut self, max_time: f64) {
        // Если текущие данные выходят за пределы отображаемого диапазона,
        // сдвигаем отображение чтобы показывать последние данные
        if max_time > self.current_view_end {
            self.current_view_end = max_time;
        }
        
        // Если масштаб изменился, но данных еще мало, показываем от 0 до масштаба
        let scale_seconds = self.time_scale.get_seconds();
        if max_time < scale_seconds {
            self.current_view_end = scale_seconds;
        }
    }

    /// Увеличить масштаб (меньше времени на экране)
    pub fn zoom_in(&mut self) {
        let old_scale = self.time_scale;
        self.time_scale = self.time_scale.prev_scale();
        
        // При изменении масштаба сохраняем конец отображения, но меняем ширину
        let scale_change = self.time_scale.get_seconds() - old_scale.get_seconds();
        self.current_view_end -= scale_change;
        
        // Не позволяем уйти в отрицательное время
        if self.current_view_end < self.time_scale.get_seconds() {
            self.current_view_end = self.time_scale.get_seconds();
        }
    }

    /// Уменьшить масштаб (больше времени на экране)
    pub fn zoom_out(&mut self) {
        let old_scale = self.time_scale;
        self.time_scale = self.time_scale.next_scale();
        
        // При изменении масштаба сохраняем конец отображения, но меняем ширину
        let scale_change = self.time_scale.get_seconds() - old_scale.get_seconds();
        self.current_view_end += scale_change;
    }

    /// Получить текущий масштаб
    pub fn get_time_scale(&self) -> TimeScale {
        self.time_scale
    }
}