use crate::tcpdiag::{gather_sockets, DiagWithInode, TCP_STATE, TCPInfo};
use std::vec::Vec;
use std::collections::VecDeque;
use ratatui::widgets::TableState;
use std::collections::HashMap;
use trust_dns_resolver::Resolver;
use trust_dns_resolver::config::*;
use std::net::IpAddr;
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::{RwLock, Arc};
use std::thread;
use std::time::Duration;


// TODO: seperate config?
pub const HISTORY_RETENTION: usize = 30;

pub struct SocketHistory {
    pub send_bps: VecDeque<u64>,
    pub recv_bps: VecDeque<u64>,
    pub send_bytes: VecDeque<u64>,
    pub recv_bytes: VecDeque<u64>,
    pub packet_loss: VecDeque<u32>,
    pub congestion_window: VecDeque<u64>,
}

impl SocketHistory {
    fn new(size: usize, tci: &TCPInfo) -> SocketHistory {
        let mut history = SocketHistory {
            send_bps: VecDeque::with_capacity(size),
            recv_bps: VecDeque::with_capacity(size),
            send_bytes: VecDeque::with_capacity(size),
            recv_bytes: VecDeque::with_capacity(size),
            packet_loss: VecDeque::with_capacity(size),
            congestion_window: VecDeque::with_capacity(size),
        };

        // Insert current segment counts to avoid burst rate when first ran
        history.send_bps.push_front(0);
        history.recv_bps.push_front(0);
        history.send_bytes.push_front(tci.tcpi_bytes_sent);
        history.recv_bytes.push_front(tci.tcpi_bytes_received);
        history.packet_loss.push_front(0);
        history.congestion_window.push_front(0);
        history
    }
}

fn is_bps(n: f64) -> bool { n < 1000.0 }
fn is_kbps(n: f64) -> bool { n >= 1000.0 && n < 1000000.0 }
fn is_mbps(n: f64) -> bool { n >= 1000000.0 }

fn friendly_transfer_str(rate: u64) -> String {
    let rate = rate as f64;
    match rate {
        n if is_bps(n) => { format!("{} bps", rate) }
        n if is_kbps(n) => { format!("{:.2} kbps", rate/1000.0) }
        n if is_mbps(n) => { format!("{:.2} mbps", rate/1000000.0) }
        _ => "".to_string()
    }
}

fn lookup_addr(ipaddr: IpAddr) -> String {
    let resolver = Resolver::new(ResolverConfig::default(), ResolverOpts::default()).unwrap();
    let response = match resolver.reverse_lookup(ipaddr) {
        Ok(record) => { record.iter().next().unwrap().to_ascii() } 
        Err(_) => { ipaddr.to_string() }
    };
    response
}

pub struct StatefulTable {
    pub state: TableState,
    pub items: Vec<Vec<String>>,
    pub sockets: Vec<DiagWithInode>,
    pub history: HashMap<u32, SocketHistory>,
    name_channel: Sender<IpAddr>,
    name_lookups: Arc<RwLock<HashMap<IpAddr, String>>>,
}


impl<'a> StatefulTable {
    pub fn new() -> StatefulTable {
        // non-blocking DNS resolution will be hanlded in a seperate thread with a channel
        // setup to receive requests that aren't already in our name hashmap. This this will be
        // "detached" and never joined. Im not sure if this matters or not since when the parent
        // PID dies so does the thread?
        let (chan_tx, chan_rx) = mpsc::channel();
        let name_map = Arc::new(RwLock::new(HashMap::new()));
        let thread_name_map = name_map.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(200));
            match chan_rx.try_recv() {
                Ok(ipaddr) => { 
                    let response = lookup_addr(ipaddr);
                    thread_name_map.write().unwrap().insert(ipaddr, response);
                }
                Err(TryRecvError::Disconnected) => {
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
        });

        let sockets: Vec<DiagWithInode> = gather_sockets();
        let new_table = StatefulTable {
            state: TableState::default(),
            items: Vec::new(),
            sockets: sockets,
            history: HashMap::new(),
            name_channel: chan_tx.clone(),
            name_lookups: name_map.clone(),
        };

        // TODO: this is maybe not a great pattern. we use data bound to the struct to generate the
        // string which is also bound to the struct, so we'd have to make this table mutable to be
        // able to assign items to the existing struct after creation.
        new_table
    }

    pub fn refresh(&mut self) {
        let sockets: Vec<DiagWithInode> = gather_sockets();
        let items = self.gen_socket_string_vector();
        self.sockets = sockets;
        self.items = items;
    }

    fn gen_socket_string_vector(&mut self) -> Vec<Vec<String>> {
        let mut result: Vec<Vec<String>> = Vec::new();
        for sock in &self.sockets {
            let tcp_info = sock.info.as_ref().unwrap();
            let history_data = self.history.entry(sock.inode).or_insert(SocketHistory::new(HISTORY_RETENTION, &tcp_info));
            let send_bps = tcp_info.tcpi_bytes_sent - history_data.send_bytes[0]; 
            let recv_bps = tcp_info.tcpi_bytes_received - history_data.recv_bytes[0];

            // dont want to divide by zero
            let packet_loss = match tcp_info.tcpi_data_segs_out {
                0 => 0,
                _ => tcp_info.tcpi_total_retrans / tcp_info.tcpi_data_segs_out
            };

            if send_bps == tcp_info.tcpi_bytes_sent {
                history_data.send_bps.push_front(0);
            } else {
                history_data.send_bps.push_front(send_bps);
            }
            if recv_bps == tcp_info.tcpi_bytes_received {
                history_data.recv_bps.push_front(0);
            } else {
                history_data.recv_bps.push_front(recv_bps);
            }

            history_data.send_bytes.push_front(tcp_info.tcpi_bytes_sent);
            history_data.recv_bytes.push_front(tcp_info.tcpi_bytes_received);
            history_data.packet_loss.push_front(packet_loss);
            history_data.congestion_window.push_front(tcp_info.tcpi_snd_cwnd as u64);

            // Remove extra items if we are past capacity
            history_data.send_bytes.truncate(HISTORY_RETENTION);
            history_data.recv_bytes.truncate(HISTORY_RETENTION);
            history_data.send_bps.truncate(HISTORY_RETENTION);
            history_data.recv_bps.truncate(HISTORY_RETENTION);
            history_data.packet_loss.truncate(HISTORY_RETENTION);
            history_data.congestion_window.truncate(HISTORY_RETENTION);

            let src_name = match self.name_lookups.read().unwrap().get(&sock.src.ip()) {
                Some(record) => record.to_string(), // why do i need this here?
                None => { 
                    self.name_channel.send(sock.src.ip()).unwrap();
                    sock.src.ip().to_string()
                }
            };
            let dst_name = match self.name_lookups.read().unwrap().get(&sock.dst.ip()) {
                Some(record) => record.to_string(), // why do i need this here?
                None => { 
                    self.name_channel.send(sock.dst.ip()).unwrap();
                    sock.dst.ip().to_string()
                }
            };

            let entry = vec![
                format!("{}:{}", src_name, sock.src.port().to_string()),
                format!("{}:{}", dst_name, sock.dst.port().to_string()),
                TCP_STATE::from_u8(tcp_info.tcpi_state).to_string(),
                friendly_transfer_str(history_data.send_bps[0]),
                friendly_transfer_str(history_data.recv_bps[0]),
                format!("{}%", history_data.packet_loss[0].to_string()),
            ];
            result.push(entry);
        }
        result 
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_is_bps() {
    use super::is_bps;

    assert_eq!(is_bps(901.0), true);
    assert_eq!(is_bps(1001.0), false);
  }

  #[test]
  fn test_is_kbps() {
    use super::is_kbps;

    assert_eq!(is_kbps(901.0), false);
    assert_eq!(is_kbps(1001.0), true);
  }

  #[test]
  fn test_is_mbps() {
    use super::is_mbps;

    assert_eq!(is_mbps(1111901.0), true);
    assert_eq!(is_mbps(1001.0), false);
  }

  #[test]
  fn test_friendly_transfer_str() {
      use super::friendly_transfer_str;
      
      assert_eq!(friendly_transfer_str(1111901), "1.11 mbps");
      assert_eq!(friendly_transfer_str(1119901), "1.12 mbps");
      assert_eq!(friendly_transfer_str(999), "999 bps");
      assert_eq!(friendly_transfer_str(999), "999 bps");
      assert_eq!(friendly_transfer_str(9911), "9.91 kbps");
      assert_eq!(friendly_transfer_str(9999), "10.00 kbps");
      assert_eq!(friendly_transfer_str(112233), "112.23 kbps");
  }
}
