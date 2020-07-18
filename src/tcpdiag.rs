#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
use nell::Message;
use nell::Netlink;
use nell::Socket;
use nell::Family;
use nell::ffi::diag::{inet_diag_msg, inet_diag_req_v2, SOCK_DIAG_BY_FAMILY, INET_DIAG_INFO};
use nell::ffi::core::{NLM_F_DUMP, NLM_F_REQUEST, IPPROTO_TCP, AF_INET};
use nell::sys::Bytes;
use nell::err::Invalid;
use std::net::{SocketAddr, IpAddr};
use std::mem::transmute;
use std::convert::TryFrom;
use std::vec::Vec;
use std::string::String;

#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct TCPInfo {
    pub tcpi_state:           u8,
    pub tcpi_ca_state:        u8,
    pub tcpi_retransmits:     u8,
    pub tcpi_probes:          u8,
    pub tcpi_backoff:         u8,
    pub tcpi_options:         u8,
    pub _bitfield_1:          [u8; 2usize],
    pub tcpi_rto:             u32,
    pub tcpi_ato:             u32,
    pub tcpi_snd_mss:         u32,
    pub tcpi_rcv_mss:         u32,
    pub tcpi_unacked:         u32,
    pub tcpi_sacked:          u32,
    pub tcpi_lost:            u32,
    pub tcpi_retrans:         u32,
    pub tcpi_fackets:         u32,
    pub tcpi_last_data_sent:  u32,
    pub tcpi_last_ack_sent:   u32,
    pub tcpi_last_data_recv:  u32,
    pub tcpi_last_ack_recv:   u32,
    pub tcpi_pmtu:            u32,
    pub tcpi_rcv_ssthresh:    u32,
    pub tcpi_rtt:             u32,
    pub tcpi_rttvar:          u32,
    pub tcpi_snd_ssthresh:    u32,
    pub tcpi_snd_cwnd:        u32,
    pub tcpi_advmss:          u32,
    pub tcpi_reordering:      u32,
    pub tcpi_rcv_rtt:         u32,
    pub tcpi_rcv_space:       u32,
    pub tcpi_total_retrans:   u32,
    pub tcpi_pacing_rate:     u64,
    pub tcpi_max_pacing_rate: u64,
    pub tcpi_bytes_acked:     u64,
    pub tcpi_bytes_received:  u64,
    pub tcpi_segs_out:        u32,
    pub tcpi_segs_in:         u32,
    pub tcpi_notsent_bytes:   u32,
    pub tcpi_min_rtt:         u32,
    pub tcpi_data_segs_in:    u32,
    pub tcpi_data_segs_out:   u32,
    pub tcpi_delivery_rate:   u64,
    pub tcpi_busy_time:       u64,
    pub tcpi_rwnd_limited:    u64,

    pub tcpi_sndbuf_limited:  u64,
    pub tcpi_delivered:       u32,
    pub tcpi_delivered_ce:    u32,
    pub tcpi_bytes_sent:      u64,
    pub tcpi_bytes_retrains:  u64,
    
    /*
    pub tcpi_dsack_dups:      u32,
    pub tcpi_reord_seen:      u32,
    pub tcpi_rcv_ooopack:     u32,
    pub tcpi_send_wnd:        u32,
    */
}

unsafe impl Bytes for TCPInfo{}

#[derive(Debug)]
pub struct DiagWithInode<T = TCPInfo> {
    family: u8,
    pub src:    SocketAddr,
    pub dst:    SocketAddr,
    state:  u8,
    pub inode:  u32,
    pub info:   Option<T>,
}

fn diag_with_node(msg: &Message<inet_diag_msg>) -> Result<DiagWithInode, Invalid> {
    let src  = addr(msg.idiag_family, &msg.id.idiag_src, msg.id.idiag_sport)?;
    let dst  = addr(msg.idiag_family, &msg.id.idiag_dst, msg.id.idiag_dport)?;
    let info = msg.info();

    Ok(DiagWithInode {
        family: msg.idiag_family,
        src:    src,
        dst:    dst,
        state:  msg.idiag_state,
        info:   info,
        inode:  msg.idiag_inode,
    })
}

fn addr(family: u8, addr: &[u32; 4], port: u16) -> Result<SocketAddr, Invalid> {
    let octets: &[u8; 16] = unsafe { transmute(addr) };
    Ok(SocketAddr::new(match family {
        AF_INET  => IpAddr::from(<[u8;  4]>::try_from(&octets[..4])?),
        AF_INET6 => IpAddr::from(<[u8; 16]>::try_from(&octets[..])?),
        family   => return Err(Invalid::Family(family)),
    }, port.to_be()))
}

pub fn gather_sockets() -> Vec<DiagWithInode> {
    let mut socket = Socket::new(Family::INET_DIAG).unwrap();
    let mut msg = Message::<inet_diag_req_v2>::new(SOCK_DIAG_BY_FAMILY);
    msg.set_flags(NLM_F_REQUEST | NLM_F_DUMP);
    msg.sdiag_family = AF_INET;
    msg.sdiag_protocol = IPPROTO_TCP;
    msg.idiag_states = !0;
    msg.idiag_ext = 1 << (INET_DIAG_INFO as u8 - 1);

    socket.send(&msg).unwrap();

    let mut sockets: Vec<DiagWithInode> = Vec::new();
    while let Netlink::Msg(msg) = socket.recv::<inet_diag_msg>().unwrap() {
        let sockdiag = diag_with_node(&msg).unwrap();
        match &sockdiag.info {
            Some(info) => {
                // LISTEN state is pretty pointless for this. It really only serves as a receive
                // queue to create NEW sockets for clients. We will get the info from those newly
                // created sockets, not the LISTEN one.
                if info.tcpi_state != 10 {
                    sockets.push(sockdiag)
                }
            },
            None => continue
        }
    }
    return sockets;
}

pub enum TCP_STATE {
    UNKNOWN,
    ESTABLISHED,
    SYN_SENT,
    SYN_RECV,
    FIN_WAIT1,
    FIN_WAIT2,
    TIME_WAIT,
    CLOSE,
    CLOSE_WAIT,
    LAST_ACK,
    LISTEN,
    CLOSING,
    NEW_SYN_REC,
    MAX_STATES
}

impl TCP_STATE {
    pub fn from_u8(state: u8) -> TCP_STATE {
        match state {
            1 => TCP_STATE::ESTABLISHED,
            2 => TCP_STATE::SYN_SENT,
            3 => TCP_STATE::SYN_RECV,
            4 => TCP_STATE::FIN_WAIT1,
            5 => TCP_STATE::FIN_WAIT2,
            6 => TCP_STATE::TIME_WAIT,
            7 => TCP_STATE::CLOSE,
            8 => TCP_STATE::CLOSE_WAIT,
            9 => TCP_STATE::LAST_ACK,
            10 => TCP_STATE::LISTEN,
            11 => TCP_STATE::CLOSING,
            12 => TCP_STATE::NEW_SYN_REC,
            13 => TCP_STATE::MAX_STATES,
            _ => panic!("dont do it")
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            TCP_STATE::UNKNOWN => String::from("UNKNOWN"),
            TCP_STATE::ESTABLISHED => String::from("ESTABLISHED"),
            TCP_STATE::SYN_SENT => String::from("SYN_SENT"),
            TCP_STATE::SYN_RECV => String::from("SYN_RECV"),
            TCP_STATE::FIN_WAIT1 => String::from( "FIN_WAIT1"),
            TCP_STATE::FIN_WAIT2 => String::from( "FIN_WAIT2"),
            TCP_STATE::TIME_WAIT => String::from( "TIME_WAIT"),
            TCP_STATE::CLOSE => String::from("CLOSE"),
            TCP_STATE::CLOSE_WAIT => String::from("CLOSE_WAIT"),
            TCP_STATE::LAST_ACK => String::from("LAST_ACK"),
            TCP_STATE::LISTEN => String::from("LISTEN"),
            TCP_STATE::CLOSING => String::from("CLOSING"),
            TCP_STATE::NEW_SYN_REC => String::from("NEW_SYN_REC"),
            TCP_STATE::MAX_STATES => String::from("MAX_STATES")
        }
    }
}
