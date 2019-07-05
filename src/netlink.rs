#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use libc;

// define the struct, const, etc needed by 
// netlink operations

pub type __s8 = libc::c_char;
pub type __u8 = libc::c_uchar;
pub type __s16 = libc::c_short;
pub type __u16 = libc::c_ushort;
pub type __s32 = libc::c_int;
pub type __u32 = libc::c_uint;
pub type __s64 = libc::c_longlong;
pub type __u64 = libc::c_ulonglong;

// we need ifaddrmsg, ifinfomasg, rtmsg
// we need some constant

pub const RTM_BASE: libc::c_ushort = 16;
pub const RTM_NEWLINK: libc::c_ushort = 16;
pub const RTM_DELLINK: libc::c_ushort = 17;
pub const RTM_GETLINK: libc::c_ushort = 18;
pub const RTM_SETLINK: libc::c_ushort = 19;
pub const RTM_NEWADDR: libc::c_ushort = 20;
pub const RTM_DELADDR: libc::c_ushort = 21;
pub const RTM_GETADDR: libc::c_ushort = 22;
pub const RTM_NEWROUTE: libc::c_ushort = 24;
pub const RTM_DELROUTE: libc::c_ushort = 25;
pub const RTM_GETROUTE: libc::c_ushort = 26;
pub const RTM_NEWNEIGH: libc::c_ushort = 28;
pub const RTM_DELNEIGH: libc::c_ushort = 29;
pub const RTM_GETNEIGH: libc::c_ushort = 30;
pub const RTM_NEWRULE: libc::c_ushort = 32;
pub const RTM_DELRULE: libc::c_ushort = 33;
pub const RTM_GETRULE: libc::c_ushort = 34;
pub const RTM_NEWQDISC: libc::c_ushort = 36;
pub const RTM_DELQDISC: libc::c_ushort = 37;
pub const RTM_GETQDISC: libc::c_ushort = 38;
pub const RTM_NEWTCLASS: libc::c_ushort = 40;
pub const RTM_DELTCLASS: libc::c_ushort = 41;
pub const RTM_GETTCLASS: libc::c_ushort = 42;
pub const RTM_NEWTFILTER: libc::c_ushort = 44;
pub const RTM_DELTFILTER: libc::c_ushort = 45;
pub const RTM_GETTFILTER: libc::c_ushort = 46;
pub const RTM_NEWACTION: libc::c_ushort = 48;
pub const RTM_DELACTION: libc::c_ushort = 49;
pub const RTM_GETACTION: libc::c_ushort = 50;
pub const RTM_NEWPREFIX: libc::c_ushort = 52;
pub const RTM_GETMULTICAST: libc::c_ushort = 58;
pub const RTM_GETANYCAST: libc::c_ushort = 62;
pub const RTM_NEWNEIGHTBL: libc::c_ushort = 64;
pub const RTM_GETNEIGHTBL: libc::c_ushort = 66;
pub const RTM_SETNEIGHTBL: libc::c_ushort = 67;
pub const RTM_NEWNDUSEROPT: libc::c_ushort = 68;
pub const RTM_NEWADDRLABEL: libc::c_ushort = 72;
pub const RTM_DELADDRLABEL: libc::c_ushort = 73;
pub const RTM_GETADDRLABEL: libc::c_ushort = 74;
pub const RTM_GETDCB: libc::c_ushort = 78;
pub const RTM_SETDCB: libc::c_ushort = 79;
pub const RTM_NEWNETCONF: libc::c_ushort = 80;
pub const RTM_GETNETCONF: libc::c_ushort = 82;
pub const RTM_NEWMDB: libc::c_ushort = 84;
pub const RTM_DELMDB: libc::c_ushort = 85;
pub const RTM_GETMDB: libc::c_ushort = 86;
pub const RTM_NEWNSID: libc::c_ushort = 88;
pub const RTM_DELNSID: libc::c_ushort = 89;
pub const RTM_GETNSID: libc::c_ushort = 90;
pub const RTM_NEWSTATS: libc::c_ushort = 92;
pub const RTM_GETSTATS: libc::c_ushort = 94;
pub const RTM_NEWCACHEREPORT: libc::c_ushort = 96;
pub const RTM_NEWCHAIN: libc::c_ushort = 100;
pub const RTM_DELCHAIN: libc::c_ushort = 101;
pub const RTM_GETCHAIN: libc::c_ushort = 102;
pub const __RTM_MAX: libc::c_ushort = 103;

pub struct rtattr {
	rta_len: libc::c_ushort,
	rta_type: libc::c_ushort,
}

pub struct rtmsg {
	rtm_family: libc::c_uchar,
	rtm_dst_len: libc::c_uchar,
	rtm_src_len: libc::c_uchar,
	rtm_tos: libc::c_uchar,
	rtm_table: libc::c_uchar,
	rtm_protocol: libc::c_uchar,
	rtm_scope: libc::c_uchar,
	rtm_type: libc::c_uchar,
	rtm_flags: libc::c_uint,
}

// rtm_type c_uchar
pub const RTN_UNSPEC: libc::c_uchar = 0;
pub const RTN_UNICAST: libc::c_uchar = 1;
pub const RTN_LOCAL: libc::c_uchar = 2;
pub const RTN_BROADCAST: libc::c_uchar = 3;
pub const RTN_ANYCAST: libc::c_uchar = 4;
pub const RTN_MULTICAST: libc::c_uchar = 5;
pub const RTN_BLACKHOLE: libc::c_uchar = 6;
pub const RTN_UNREACHABLE: libc::c_uchar = 7;
pub const RTN_PROHIBIT: libc::c_uchar = 8;
pub const RTN_THROW: libc::c_uchar = 9;
pub const RTN_NAT: libc::c_uchar = 10;
pub const RTN_XRESOLVE: libc::c_uchar = 11;
pub const __RTN_MAX: libc::c_uchar = 12;

// rtm_protocol c_uchar
pub const RTPROTO_UNSPEC: libc::c_uchar = 0;
pub const RTPROTO_REDIRECT: libc::c_uchar = 1;
pub const RTPROTO_KERNEL: libc::c_uchar = 2;
pub const RTPROTO_BOOT: libc::c_uchar = 3;
pub const RTPROTO_STATIC: libc::c_uchar = 4;

pub const RTPROTO_GATED: libc::c_uchar = 8;
pub const RTPROTO_RA: libc::c_uchar = 9;
pub const RTPROTO_MRT: libc::c_uchar = 10;
pub const RTPROTO_ZEBRA: libc::c_uchar = 11;
pub const RTPROTO_BIRD: libc::c_uchar = 12;
pub const RTPROTO_DNROUTED: libc::c_uchar = 13;
pub const RTPROTO_XORP: libc::c_uchar = 14;
pub const RTPROTO_NTK: libc::c_uchar = 15;
pub const RTPROTO_DHCP: libc::c_uchar = 16;
pub const RTPROTO_MROUTED: libc::c_uchar = 17;
pub const RTPROTO_BABEL: libc::c_uchar = 42;
pub const RTPROTO_BGP: libc::c_uchar = 186;
pub const RTPROTO_ISIS: libc::c_uchar = 187;
pub const RTPROTO_OSPF: libc::c_uchar = 188;
pub const RTPROTO_RIP: libc::c_uchar = 189;
pub const RTPROTO_EIGRP: libc::c_uchar = 192;

//rtm_scope c_uchar
pub const RT_SCOPE_UNIVERSE: libc::c_uchar = 0;
pub const RT_SCOPE_SITE: libc::c_uchar = 200;
pub const RT_SCOPE_LINK: libc::c_uchar = 253;
pub const RT_SCOPE_HOST: libc::c_uchar = 254;
pub const RT_SCOPE_NOWHERE: libc::c_uchar = 255;

// rtm_flags c_uint
pub const RTM_F_NOTIFY: libc::c_uint = 0x100;
pub const RTM_F_CLONED: libc::c_uint = 0x200;
pub const RTM_F_EQUALIZE: libc::c_uint = 0x400;
pub const RTM_F_PREFIX: libc::c_uint = 0x800;
pub const RTM_F_LOOKUP_TABLE: libc::c_uint = 0x1000;
pub const RTM_F_FIB_MATCH: libc::c_uint = 0x2000;

// table identifier
pub const RT_TABLE_UNSPEC: libc::c_uint = 0;
pub const RT_TABLE_COMPAT: libc::c_uint = 252;
pub const RT_TABLE_DEFAULT: libc::c_uint = 253;
pub const RT_TABLE_MAIN: libc::c_uint = 254;
pub const RT_TABLE_LOCAL: libc::c_uint = 255;
pub const RT_TABLE_MAX: libc::c_uint = 0xffffffff;

// rat_type c_ushort
pub const RTA_UNSPEC: libc::c_ushort = 0;
pub const RTA_DST: libc::c_ushort = 1;
pub const RTA_SRC: libc::c_ushort = 2;
pub const RTA_IIF: libc::c_ushort = 3;
pub const RTA_OIF: libc::c_ushort = 4;
pub const RTA_GATEWAY: libc::c_ushort = 5;
pub const RTA_PRIORITY: libc::c_ushort = 6;
pub const RTA_PREFSRC: libc::c_ushort = 7;
pub const RTA_METRICS: libc::c_ushort = 8;
pub const RTA_MULTIPATH: libc::c_ushort = 9;
pub const RTA_PROTOINFO: libc::c_ushort = 10;
pub const RTA_FLOW: libc::c_ushort = 11;
pub const RTA_CACHEINFO: libc::c_ushort = 12;
pub const RTA_SESSION: libc::c_ushort = 13;
pub const RTA_MP_ALGO: libc::c_ushort = 14;
pub const RTA_TABLE: libc::c_ushort = 15;
pub const RTA_MARK: libc::c_ushort = 16;
pub const RTA_MFC_STATS: libc::c_ushort = 17;
pub const RTA_VIA: libc::c_ushort = 18;
pub const RTA_NEWDST: libc::c_ushort = 19;
pub const RTA_PREF: libc::c_ushort = 20;
pub const RTA_ENCAP_TYPE: libc::c_ushort = 21;
pub const RTA_ENCAP: libc::c_ushort = 22;
pub const RTA_EXPIRES: libc::c_ushort = 23;
pub const RTA_PAD: libc::c_ushort = 24;
pub const RTA_UID: libc::c_ushort = 25;
pub const RTA_TTL_PROPAGATE: libc::c_ushort = 26;
pub const RTA_IP_PROTO: libc::c_ushort = 27;
pub const RTA_SPORT: libc::c_ushort = 28;
pub const RTA_DPORT: libc::c_ushort = 29;
pub const __RTA_MAX: libc::c_ushort = 30;
