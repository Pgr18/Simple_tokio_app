use crate::com_port::DataPacket;

pub struct DataProcessor {
    max_points: usize,
    channel_a: Vec<(u64, f64)>,
    channel_b: Vec<(u64, f64)>,
    channel_c: Vec<(u64, f64)>,
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        Self {
            max_points,
            channel_a: Vec::with_capacity(max_points),
            channel_b: Vec::with_capacity(max_points),
            channel_c: Vec::with_capacity(max_points),
        }
    }

    pub fn add_packet(&mut self, packet: DataPacket) {
        // Обрабатываем каждый канал последовательно
        self.process_channel_a(packet.timestamp, packet.channel_a);
        self.process_channel_b(packet.timestamp, packet.channel_b);
        self.process_channel_c(packet.timestamp, packet.channel_c);
    }

    fn process_channel_a(&mut self, timestamp: u64, value: f64) {
        self.channel_a.push((timestamp, value));
        if self.channel_a.len() > self.max_points {
            self.channel_a.remove(0);
        }
    }

    fn process_channel_b(&mut self, timestamp: u64, value: f64) {
        self.channel_b.push((timestamp, value));
        if self.channel_b.len() > self.max_points {
            self.channel_b.remove(0);
        }
    }

    fn process_channel_c(&mut self, timestamp: u64, value: f64) {
        self.channel_c.push((timestamp, value));
        if self.channel_c.len() > self.max_points {
            self.channel_c.remove(0);
        }
    }

    pub fn get_channel_a(&self) -> &[(u64, f64)] {
        &self.channel_a
    }

    pub fn get_channel_b(&self) -> &[(u64, f64)] {
        &self.channel_b
    }

    pub fn get_channel_c(&self) -> &[(u64, f64)] {
        &self.channel_c
    }

    pub fn clear(&mut self) {
        self.channel_a.clear();
        self.channel_b.clear();
        self.channel_c.clear();
    }
}