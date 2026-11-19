//! Layout, version and symbol checks that pin the ABI of ADR-0087.

use core::mem::{align_of, offset_of, size_of};

use umicraft_modapi_sys::*;

#[cfg(target_pointer_width = "64")]
#[test]
fn layouts_are_stable_on_64_bit() {
    assert_eq!((size_of::<UmiStr>(), align_of::<UmiStr>()), (16, 8));
    assert_eq!(offset_of!(UmiStr, ptr), 0);
    assert_eq!(offset_of!(UmiStr, len), 8);

    assert_eq!((size_of::<UmiVersion>(), align_of::<UmiVersion>()), (4, 2));
    assert_eq!(offset_of!(UmiVersion, major), 0);
    assert_eq!(offset_of!(UmiVersion, minor), 2);

    assert_eq!(size_of::<UmiStatus>(), 4);
    assert_eq!(size_of::<UmiModHandle>(), 4);
    assert_eq!(size_of::<UmiEventId>(), 4);
    assert_eq!(size_of::<UmiMode>(), 4);
    assert_eq!(size_of::<UmiLevel>(), 4);

    assert_eq!(
        (size_of::<UmiEventHeader>(), align_of::<UmiEventHeader>()),
        (12, 4)
    );
    assert_eq!(offset_of!(UmiEventHeader, size), 0);
    assert_eq!(offset_of!(UmiEventHeader, id), 4);
    assert_eq!(offset_of!(UmiEventHeader, flags), 8);

    for (size, align, header, tick) in [
        (
            size_of::<UmiPreTick>(),
            align_of::<UmiPreTick>(),
            offset_of!(UmiPreTick, header),
            offset_of!(UmiPreTick, tick),
        ),
        (
            size_of::<UmiPostTick>(),
            align_of::<UmiPostTick>(),
            offset_of!(UmiPostTick, header),
            offset_of!(UmiPostTick, tick),
        ),
    ] {
        assert_eq!((size, align, header, tick), (24, 8, 0, 16));
    }

    assert_eq!((size_of::<UmiHost>(), align_of::<UmiHost>()), (32, 8));
    assert_eq!(offset_of!(UmiHost, size), 0);
    assert_eq!(offset_of!(UmiHost, version), 8);
    assert_eq!(offset_of!(UmiHost, log), 16);
    assert_eq!(offset_of!(UmiHost, subscribe), 24);
}

#[test]
fn version_compatibility_rules() {
    let v = |major, minor| UmiVersion { major, minor };
    let table = [
        // (host, mod, accepted)
        (v(0, 1), v(0, 1), true),
        (v(0, 2), v(0, 1), false),
        (v(0, 1), v(0, 2), false),
        (v(0, 1), v(1, 1), false),
        (v(1, 0), v(0, 0), false),
        (v(1, 2), v(1, 2), true),
        (v(1, 3), v(1, 2), true),
        (v(1, 1), v(1, 2), false),
        (v(2, 0), v(1, 0), false),
        (v(1, 0), v(2, 0), false),
    ];
    for (host, module, expected) in table {
        assert_eq!(host.accepts(module), expected, "{host:?} vs {module:?}");
    }
    assert!(UMI_API_VERSION.accepts(UMI_API_VERSION));
    assert_eq!(UMI_API_VERSION, v(0, 1));
}

#[test]
fn symbols_are_nul_terminated_adr_names() {
    let cases: [(&[u8], &str); 3] = [
        (UMI_MOD_API_VERSION_SYMBOL, "umi_mod_api_version"),
        (UMI_MOD_INIT_SYMBOL, "umi_mod_init"),
        (UMI_MOD_SHUTDOWN_SYMBOL, "umi_mod_shutdown"),
    ];
    for (symbol, name) in cases {
        assert_eq!(symbol.last(), Some(&0));
        assert_eq!(&symbol[..symbol.len() - 1], name.as_bytes());
    }
}

#[test]
fn status_values_and_ids_match_the_contract() {
    let statuses = [
        UmiStatus::OK,
        UmiStatus::ERROR,
        UmiStatus::PANICKED,
        UmiStatus::UNSUPPORTED,
        UmiStatus::INVALID_ARGUMENT,
        UmiStatus::DENIED,
        UmiStatus::CONFLICT,
    ];
    for (i, status) in statuses.iter().enumerate() {
        assert_eq!(status.0, i as i32);
    }
    assert!(UmiStatus::OK.is_ok());
    assert!(!UmiStatus(99).is_ok());
    assert_eq!((UmiMode::OBSERVE.0, UmiMode::INTERCEPT.0), (0, 1));
    assert_eq!(
        [
            UmiLevel::ERROR,
            UmiLevel::WARN,
            UmiLevel::INFO,
            UmiLevel::DEBUG,
            UmiLevel::TRACE
        ]
        .map(|l| l.0),
        [1, 2, 3, 4, 5]
    );
    assert_eq!((UmiEventId::PRE_TICK.0, UmiEventId::POST_TICK.0), (1, 2));
    assert_eq!((UMI_EVENT_CANCELLABLE, UMI_EVENT_CANCELLED), (1, 2));
}

#[test]
fn umistr_constructors() {
    assert!(UmiStr::EMPTY.ptr.is_null());
    assert_eq!(UmiStr::EMPTY.len, 0);
    let s = UmiStr::from_static("héllo");
    assert_eq!(s.len, 6);
    assert!(!s.ptr.is_null());
}
