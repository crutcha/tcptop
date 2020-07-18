use crate::table;
use tui::layout::{Constraint, Layout, Direction, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Row, Table, Block, Borders, Chart, Dataset, Axis, GraphType, Text, Paragraph};
use tui::terminal::Frame;
use tui::backend::Backend;
use tui::symbols;
use std::collections::VecDeque;


fn vecdequeue_as_chart(rate: &VecDeque<u64>) -> [(f64, f64); table::HISTORY_RETENTION] {
    let mut chart_points = [(0.0, 0.0); table::HISTORY_RETENTION];
    for (index, value) in rate.iter().enumerate() {
        chart_points[index] = (index as f64, *value as f64);
    }
    chart_points
}

fn determine_min_max_values(rate: &VecDeque<u64>) -> [f64; 2] {
    // For now the min here will always be 0. We might want to revisit this and
    // create a more dynamic bound for each chart
    let (min, mut max) = (0u64, 0u64);
    for sample in rate {
        if sample > &max {
            max = *sample;
        }
    }
    [min as f64, max as f64]
}

pub struct CLI {
    pub overview: table::StatefulTable,
    detail_toggle: bool,
}

impl CLI {
    pub fn new() -> Self {
        Self {
            overview: table::StatefulTable::new(),
            detail_toggle: false,
        }
    }

    pub fn render<B: Backend>(&mut self, frame: &mut Frame<B>) {
        let terminal_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(frame.size().height - 1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(frame.size());
        match self.detail_toggle {
            false => self.draw_overview(frame, terminal_chunks[0]),
            true => self.draw_detail(frame, terminal_chunks[0]) 
        }

        let help_text = [
            Text::styled("<j, down>", Style::default().bg(Color::Gray).fg(Color::Blue).modifier(Modifier::BOLD)),
            Text::raw(format!(" to move down  ")),
            Text::styled("<k, up>", Style::default().bg(Color::Gray).fg(Color::Blue).modifier(Modifier::BOLD)),
            Text::raw(format!(" to move up  ")),
            Text::styled("<ENTER>", Style::default().bg(Color::Gray).fg(Color::Blue).modifier(Modifier::BOLD)),
            Text::raw(format!(" defail for selected socket  ")),
            Text::styled("<b>", Style::default().bg(Color::Gray).fg(Color::Blue).modifier(Modifier::BOLD)),
            Text::raw(format!(" back to table view  ")),
        ];
        let help = Paragraph::new(help_text.iter())
            .wrap(true);
        frame.render_widget(help, terminal_chunks[1]);
    }

    // TODO: result return here?
    pub fn on_tick(&mut self) {
        self.overview.refresh();
    }

    pub fn enter_detail_view(&mut self) {
        if self.detail_toggle == false {
            self.detail_toggle = true;
        }
    }

    pub fn exit_detail_view(&mut self) {
        if self.detail_toggle == true {
            self.detail_toggle = false;
        }
    }

    fn draw_overview<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        let rects = Layout::default()
            .constraints([Constraint::Percentage(100)].as_ref())
            .margin(0)
            .split(area);

        let selected_style = Style::default().fg(Color::Yellow).modifier(Modifier::BOLD);
        let normal_style = Style::default().fg(Color::White);
        let header = ["Source", "Dest", "State", "Send", "Recv", "Loss"];
        let rows = self.overview
            .items
            .iter()
            .map(|i| Row::StyledData(i.iter(), normal_style));
        let t = Table::new(header.iter(), rows)
            .block(Block::default().borders(Borders::ALL).title("TCPtop"))
            .header_style(Style::default().fg(Color::Red).modifier(Modifier::BOLD))
            .highlight_style(selected_style)
            .highlight_symbol(">> ")
            .widths(&[
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(20),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
            ]);
        frame.render_stateful_widget(t, rects[0], &mut self.overview.state);

    }

    fn draw_detail<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(40),
                    Constraint::Percentage(60),
                ]
                .as_ref(),
            )
            .split(area);
        self.draw_detail_stats(frame, chunks[0]);
        self.draw_detail_charts(frame, chunks[1]);
    }

    fn draw_detail_stats<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Ratio(1, 2),
                    Constraint::Ratio(1, 2),
                ]
                .as_ref(),
            )
            .split(area);
        let detail_entry = &self.overview.sockets[self.overview.state.selected().unwrap()];
        let detail_history = &self.overview.history.get(&detail_entry.inode).unwrap();
        let tcp_info = detail_entry.info.as_ref().unwrap();
        let chart_data_window = vecdequeue_as_chart(&detail_history.congestion_window);
        let chart_bounds_window = determine_min_max_values(&detail_history.congestion_window);
        let chart_labels_window = &[chart_bounds_window[0].to_string(), (chart_bounds_window[1]/2.0).to_string(), chart_bounds_window[1].to_string()];
        let text = [
            Text::styled("Src: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}:{}\n", detail_entry.src.ip().to_string(), detail_entry.src.port().to_string())),
            Text::styled("Dst: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}:{}\n", detail_entry.dst.ip().to_string(), detail_entry.dst.port().to_string())),
            Text::styled("Inode: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", detail_entry.inode)),
            Text::styled("Retransmits: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_total_retrans)),
            Text::styled("RTO: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_rto)),
            Text::styled("ATO: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_ato)),
            Text::styled("Send MSS: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_snd_mss)),
            Text::styled("Recv Mss: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_rcv_mss)),
            Text::styled("Lost: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_lost)),
            Text::styled("RTT: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_rtt)),
            Text::styled("RTT variance: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_rttvar)),
            Text::styled("Congestion window: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_snd_cwnd)),
            Text::styled("Pacing rate: ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{}\n", tcp_info.tcpi_pacing_rate)),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Socket Info")
            .title_style(Style::default().fg(Color::Magenta).modifier(Modifier::BOLD));
        let paragraph = Paragraph::new(text.iter())
            .block(block)
            .wrap(true);
        let window_dataset = [Dataset::default()
            .name("data")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Yellow))
            .graph_type(GraphType::Line)
            .data(&chart_data_window)];
        let window_chart = Chart::default()
            .block(
                Block::default()
                    .title("Window")
                    .title_style(Style::default().fg(Color::Cyan).modifier(Modifier::BOLD))
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Seconds")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds([0.0, 30.0])
                    // TODO: this should be dynamically determined
                    .labels(&["0", "15", "30"]),
            )
            .y_axis(
                Axis::default()
                    .title("Packets")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds(chart_bounds_window)
                    .labels(chart_labels_window),
            )
            .datasets(&window_dataset);
        frame.render_widget(paragraph, chunks[0]);
        frame.render_widget(window_chart, chunks[1]);
    }

    fn draw_detail_charts<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        let detail_entry = &self.overview.sockets[self.overview.state.selected().unwrap()];
        let detail_history = &self.overview.history.get(&detail_entry.inode).unwrap();
        let chart_bounds_recv = determine_min_max_values(&detail_history.recv_bps);
        let chart_bounds_send = determine_min_max_values(&detail_history.send_bps);
        let chart_labels_recv = &[chart_bounds_recv[0].to_string(), (chart_bounds_recv[1]/2.0).to_string(), chart_bounds_recv[1].to_string()];
        let chart_labels_send = &[chart_bounds_send[0].to_string(), (chart_bounds_send[1]/2.0).to_string(), chart_bounds_send[1].to_string()];
        let chart_data_recv = vecdequeue_as_chart(&detail_history.recv_bps);
        let chart_data_send = vecdequeue_as_chart(&detail_history.send_bps);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Ratio(1, 2),
                    Constraint::Ratio(1, 2),
                ]
                .as_ref(),
            )
            .split(area);
        let send_datasets = [Dataset::default()
            .name("data")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Yellow))
            .graph_type(GraphType::Line)
            .data(&chart_data_send)];
        let recv_datasets = [Dataset::default()
            .name("data")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Yellow))
            .graph_type(GraphType::Line)
            .data(&chart_data_recv)];
        let send_chart = Chart::default()
            .block(
                Block::default()
                    .title("Send")
                    .title_style(Style::default().fg(Color::Cyan).modifier(Modifier::BOLD))
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Seconds")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds([0.0, 30.0])
                    // TODO: this should be dynamically determined
                    .labels(&["0", "15", "30"]),
            )
            .y_axis(
                Axis::default()
                    .title("Rate")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds(chart_bounds_send)
                    .labels(chart_labels_send),
            )
            .datasets(&send_datasets);
        let recv_chart = Chart::default()
            .block(
                Block::default()
                    .title("Receive")
                    .title_style(Style::default().fg(Color::Cyan).modifier(Modifier::BOLD))
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds([0.0, 30.0])
                    .labels(&["0", "15", "30"]),
            )
            .y_axis(
                Axis::default()
                    .title("Rate")
                    .style(Style::default().fg(Color::Gray))
                    .labels_style(Style::default().modifier(Modifier::ITALIC))
                    .bounds(chart_bounds_recv)
                    .labels(chart_labels_recv),
            )
            .datasets(&recv_datasets);
        frame.render_widget(send_chart, chunks[0]);
        frame.render_widget(recv_chart, chunks[1]);
    }
}
