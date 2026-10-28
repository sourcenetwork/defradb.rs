//! Generated from the conformance scenarios of tests/integration/index/query_performance_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{Actions, AddCollection, AddDoc, Benchmark, NodeOpenPersistent};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

fn query_performance_simple_actions_0() -> Vec<ActionInstance<Actions>> {
    vec![
        NodeOpenPersistent {
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                    type User {
                        name:   String
                        age:    Int 
                        email:  String
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                    type IndexedUser {
                        name:   String
                        age:    Int @index
                        email:  String
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "UbYhiZzKaK", "name": "BleeANsocp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 68, "email": "IGncKyRWXe", "name": "vWsHOIPgZm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "AxhyuoHkCA", "name": "eZencueQpL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "gphOrTZQup", "name": "sLnyFJUoSx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "fMkiUieeem", "name": "AbOwbdHLTs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 59, "email": "RsUYSMOplx", "name": "luSTBqYipj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "XOXQZdikON", "name": "fkoAfLlTnW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 93, "email": "IdFaoQZUYj", "name": "olyGkIcFOr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 57, "email": "OkbeIaEgzb", "name": "RulHnKDsqs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "JaQWSlCNwk", "name": "KYYRqIKkXA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 84, "email": "DkjzcOurIi", "name": "RDAXAWwYEj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "tdJoWykRgl", "name": "NAXfNsNzpK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 23, "email": "YyAzciyupC", "name": "UZrZcSBeuo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "pgnfUZcZeD", "name": "tCBxAsQaxU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 40, "email": "lpmGWSgPbm", "name": "hXyYNfVBfi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 16, "email": "fFATPGhRpo", "name": "aMnTbgdDgQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "KQKyZdLxpu", "name": "cbMQSsNmlp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 80, "email": "LRLkzrvSBf", "name": "sXkksVLNsG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "XOGtRuhmNx", "name": "DxytPuzGIj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 39, "email": "gZtyXOEdty", "name": "yMZxFzfIeW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 36, "email": "AchgANUOuP", "name": "ylbaSjwEUT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "BsDxbmuuQB", "name": "wrpZigekwL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 9, "email": "SQAWUvTUYG", "name": "NDPonSHabE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "sqACltMsYV", "name": "ZFWHZWaPBH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "oNFClyXbbt", "name": "AKuKSrWoDq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 84, "email": "xOrJiIoLAJ", "name": "IMahKEUxWO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 16, "email": "HlURQShsnT", "name": "EviBOQKwga" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "aNlzYgrwIf", "name": "bdyPSpEfvd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "hPzvEgKWbE", "name": "RnAaIgzsLn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "JXSZbqMJtF", "name": "gHmfkHsCaz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "ZTXlqntzXD", "name": "LFATibLaAJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 5, "email": "eODHaEmpRx", "name": "kKcOwkrruS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 15, "email": "QqKTWFAtMj", "name": "MkghbqXfrE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "nlbQEtZawt", "name": "nHEGbURycn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "vTYLQBFuby", "name": "oMQRJRWtPl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "QjbCdFHjGQ", "name": "cewayLxXhP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "YrqtNnmCPI", "name": "alhpbdqhuX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 82, "email": "SRQfshYdTp", "name": "MUHWwmnHpa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "QLLUKXLxNk", "name": "GEqsXrSYCU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "snyFkLPcGx", "name": "esIxlqphKW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "JbJXKzZIYh", "name": "YKNKirTPzD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 44, "email": "jRCfFdCbyD", "name": "LOCvLNUiiD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 80, "email": "XcpZGRQmoJ", "name": "kfpxDkPwCL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 1, "email": "NcUdYFWnHB", "name": "jgoAFCsKhO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "EItQiYdDtL", "name": "ZfewzqyJOC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "AOmzIEPNqM", "name": "bCYzPDLWDq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "AXdiCOdIOq", "name": "ZuWCNkMfil" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "EmQIzqsShV", "name": "MghmLxEDPU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 93, "email": "jkYrHJDIef", "name": "beDcgtdUZq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "faQIHEiTnT", "name": "DOByaIlDkz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 65, "email": "YRSsoePgRj", "name": "DTsdcwlVWu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "SkGSfSmdBN", "name": "vugkEsTfYm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 3, "email": "SPmAyeNzGE", "name": "CZjWNIsqqM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "RgxYHBbEzu", "name": "tvGEeFZgTD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "AAKNTqEngR", "name": "nyPaGksXbM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "tUfSNLFgSe", "name": "UiFQnixYvL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "ABuLqchsIE", "name": "kWJQLINqhy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "eGzXSweJNY", "name": "bgofZkMWkN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "VOnruoiUZW", "name": "QkLJKHLwZR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 28, "email": "xiWYVnalcy", "name": "znGDbMmOop" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 25, "email": "iwRPLFMpTh", "name": "PwDZdqAveK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 37, "email": "wdxyJEputS", "name": "imFsdbuyEf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 6, "email": "JQXbjEhmAd", "name": "hXfVRInRsx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 30, "email": "XekwUzshfW", "name": "tLcUBsbgUl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 84, "email": "UMlapDSwDu", "name": "uvgyDRDUwM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 57, "email": "KLZOnGCiHs", "name": "mRNadsNoIF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "ImPfiExwlo", "name": "DsFQUnQIot" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "OvWngOsZMY", "name": "DQMTaLVtGf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 45, "email": "tcSEDOmDzZ", "name": "fhsIeRiwrv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "kbGlRLgcjR", "name": "YiRJgxPUiI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "iLeCWxfOyN", "name": "DCaSTtUcNm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 65, "email": "kqwjMBxgjE", "name": "fkpvAdQhKF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 30, "email": "TOVDtBkNmE", "name": "qqmIVxufZw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "dnBNqLIDSn", "name": "wTcRBCpzjN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 57, "email": "IfIdzsGJuv", "name": "JnWWrAArah" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "MBAsmPxDbx", "name": "ezTfgNvEQb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "pNJoPrbEuE", "name": "czNUHWQbRW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 2, "email": "vdvmSmzBhz", "name": "XZnuGMfNXu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 45, "email": "dtjsXzsEkJ", "name": "NROkRVzptD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "cgGjXmoFOK", "name": "VXIPfiMdbm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 73, "email": "vQEHstTNKt", "name": "lbSlwsTGez" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "FDrITQSicB", "name": "pegqxiajbP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "HBQMvRhwCP", "name": "FMypffIZYL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "vbzTBmlGLl", "name": "UeidmSWOdh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "qijVsHSNGM", "name": "LaDDADOuig" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "oANGdwTTYc", "name": "CIzqJOdDcA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "eMujXWvCvW", "name": "KAEcnbAnOI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 22, "email": "xxErkOFFNF", "name": "vzHDEWziSF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 44, "email": "JzXFnaADBm", "name": "LFwutemweM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 49, "email": "gCWwdiwoMC", "name": "iyQkiTwMGU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "TdeQKiSuRn", "name": "esqvDMJuKd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "bXszfoujsH", "name": "ToiSepRoYd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "mCckSuWdJY", "name": "zuLiJoGQAg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "GQUlNQYJGq", "name": "szYFZOqUTM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "hTgQXBbcoU", "name": "FGMIMYlrex" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 36, "email": "ZHZSuPsnIJ", "name": "kDsCWOytbz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 42, "email": "URGjTlAgRE", "name": "jXLqKmevGB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 22, "email": "jpplgMrNow", "name": "eIIXkCgskr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "ULFULycnXl", "name": "urGqykONXl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 49, "email": "KoTicLTRBa", "name": "mtZvidPbvT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 50, "email": "vGruHVrjTP", "name": "KwJjvFkONb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "hFrJxpoKtD", "name": "znrSpjmSZr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "kqnhnMRCiX", "name": "NqToZVGbMe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 20, "email": "BhDLSRPrAd", "name": "bpkxddmCWs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "BpEQDRviZg", "name": "oWoicwkSTs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 38, "email": "xRRewBrkoa", "name": "PQsGASZpGe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 14, "email": "ccpZmUgLEq", "name": "FUgrHeoolQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "qmeSqfOMEo", "name": "tsDglQFxAz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 42, "email": "FtyfJXIADH", "name": "usyKktcFMx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 52, "email": "yzBDNANAdF", "name": "bLhIDuiVBI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 91, "email": "czKcBwuQfc", "name": "MBHgkMhhtA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "LaCnSxhceY", "name": "uGowJkMysG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "kJuvoUFFvG", "name": "XZmoHtvjbL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "hdmEzkzygs", "name": "PCjpjCVxLp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "VmWfTLnoWl", "name": "ZgbitooVFv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "gblGIQmavy", "name": "rUJqciWKHe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 31, "email": "fbNzaxCzVe", "name": "luOufmfKqE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "yxtRFXWvmL", "name": "KrjOoSogJb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "uoxOSPdNwb", "name": "FDcAWgGkVX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 66, "email": "gTBiHQzYJJ", "name": "ySJGrzAUtt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "koLgKRhtxD", "name": "eNjYFffAAn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "HfrcGDUyFE", "name": "fchBRkCoMG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "ULoVoktsvG", "name": "BqJxwoEjGs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "jpHDDzILLJ", "name": "zMVHwFNgNz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "mTXdNnrMod", "name": "NjMMJDpALA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "sToirbYjFE", "name": "IqyAnPDmiw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 38, "email": "wpsqRzOVNC", "name": "ZztRpUgqMM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "AIIgxZMASf", "name": "FxbthclgvI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "ZkoUeHNCHu", "name": "rqutukUfuA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "ZwXXUHXRhA", "name": "wgkCRDSLzn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "rItJTIqALB", "name": "EjzAcLQxol" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "jXLcKTOLkT", "name": "bScwfhVRNa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "mguZwKaDnA", "name": "IqNmDDwOdV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "PfQAfZgAnz", "name": "UylWPAZSOm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "bkpDzNiauv", "name": "yJjttALsrk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 5, "email": "MndsWxXJDd", "name": "IkqBmfPZFk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 2, "email": "nJAlsmmZHE", "name": "cvgZDcohQf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "LAGbXNVWkX", "name": "OvXgLKczvB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "TYEIedbqiS", "name": "PBExYmfVtz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "IrUpNqUXAe", "name": "GhgfYDzRoa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "dxdghDQanM", "name": "Uvmfwdfpqu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "uFeqlOjjFt", "name": "fhDNPVWRRI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "NCZhufejXd", "name": "GvVHMDEDKi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 25, "email": "QqmvIWOomI", "name": "FUuOriwvhs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "vEWTWUjLBO", "name": "LiHhDSeVFI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 91, "email": "CGuYBjxIyx", "name": "xmuzJopUPV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 87, "email": "LywyEXUNzt", "name": "enbOGHJUij" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "zYiUDdFcCY", "name": "cMIzZmHRaS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 62, "email": "nvKKwXjUcx", "name": "ssHDNZjtev" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "ZQMcbGlfLU", "name": "NtnHICOWwp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 18, "email": "cCOESOBARa", "name": "yyqSQBrfXw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 2, "email": "iUSuawxXKL", "name": "zAaMCwnPzk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 20, "email": "SxaPwDhjBt", "name": "BPeLMJPrBm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "DdQtzqGFax", "name": "GhWkMEssCN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "JqYrjXqqPm", "name": "EkRfUhGoEH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 66, "email": "gvQHKCtMyC", "name": "BOMdSTrSGl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "zwmWvZGuuI", "name": "jPeReptdRs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "laCcKxRgTf", "name": "fdbTxXaYFk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 9, "email": "zfphzahjUD", "name": "kqlEeCOhgP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "eiRIxyipdf", "name": "fNecxfzfSt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 54, "email": "MbSOUOrWDQ", "name": "fKpjwdWusf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 89, "email": "HWhuYBwWnp", "name": "HENhLqPQSM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 37, "email": "sTuEJIbgGs", "name": "BqFhIXfjTr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "sHQUCPyxRK", "name": "ItJUdvkpAi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 59, "email": "NOrSzHDCVV", "name": "PEnRTYvatx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "UIsElfPNfO", "name": "kOHpatiYiY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 52, "email": "jGOJXnQDPu", "name": "fQkxrFOTPH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 15, "email": "PZEXlnbjFN", "name": "jHrsCFaptq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "DLgzEAUsfK", "name": "kULHUmhuDQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "efuAlBvIQd", "name": "XVcUkgKykN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "SqNRvHYAYm", "name": "JtQOtrSGJX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 6, "email": "yPlPgLHOcA", "name": "fXYhIHyqmG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 82, "email": "rfAhNovYlZ", "name": "ceXkTcjZgq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 5, "email": "kwrkdcTKRA", "name": "MtTbBBeFMj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "ExlfwLwVnd", "name": "bGYXalgoGx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "OTBCEYAAKF", "name": "DHmOTRQsgs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 19, "email": "ceWXJtawUY", "name": "STJhmmlKbX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 17, "email": "xzNrJdhZnQ", "name": "ZiCtETcXdZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "HnuMZkAHTK", "name": "xYSRqpEnUD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 89, "email": "ANixapIDnT", "name": "wICYkLLRqr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "IWEVkMoUGo", "name": "LtTqsuzzxK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "DcfdMKXnib", "name": "phZTmfkjWu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "VyTsShYgZN", "name": "UHraDyONYY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "QkzqoAOMfx", "name": "cymyahZQEe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 43, "email": "RYVsAZfrEp", "name": "ubtDBcHBZH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 65, "email": "gExguIrGWP", "name": "hVccmgarFe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "NzDkViosyE", "name": "pnqBBzmPdJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 95, "email": "vFKIDxTbxu", "name": "nkcbrtnecC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "iBACNoEFsU", "name": "jJJwVykoTf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "wTQnbpEOCJ", "name": "iEgMLjfshF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 98, "email": "hvLTHauQxv", "name": "oRoKtmxmTr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "DRGWarICua", "name": "dgBqYzagRx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 23, "email": "rHLPyNAPxn", "name": "fDGuvDOBCe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "aOGCppjrYT", "name": "ZNoRNXvXNF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 14, "email": "WGqLdaccUs", "name": "yxCmWetiPf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 95, "email": "lisGkKpdff", "name": "GfIGUUgWMO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "vhmgpJWbQG", "name": "tsPeCxsIsk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "YxcTWUMErM", "name": "glPQlwlsNM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "AelIsTKmUm", "name": "LrPiyhkGKW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "cSBtwIPhKz", "name": "gEmTVIxNAK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "HWRVNlWBfU", "name": "OUnlsJzmOv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "kIoRKOsguB", "name": "hweyfarEkS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "OeUpwCXpRd", "name": "EGjQQOSBuM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 93, "email": "sdJFdxIUyb", "name": "PORVcAFJJI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "eWkUdtVKzj", "name": "RwXKOuIzoc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "PhvjyMXOSI", "name": "rmTjioQzne" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "pfpFxQYhAD", "name": "eJTKsVzzwn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "TVRVDHWMsq", "name": "rQSCBTnnMG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "WityvQfWBO", "name": "jGuUGnwoWZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 20, "email": "BdZTDfDSCa", "name": "OkhbEXrtDO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 31, "email": "ddqfTiViGT", "name": "HeEWwFJogj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 31, "email": "WxFrraJaqw", "name": "dUVjPxjFHN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 9, "email": "djQTkXwgMA", "name": "tFOaUyDure" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 22, "email": "foTVSfcszx", "name": "haPnaMbwzB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "IrlbcXoxdw", "name": "MyShfsXICF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "roramUwwhS", "name": "wKKpEDyNpi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "FnPcXzfRoO", "name": "usKQpgjOkQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "EyXKkAnFJu", "name": "ATSiBIBgqz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 53, "email": "GvYaFAAZbn", "name": "ZBhsoGzGVO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 9, "email": "DBzhSHveOu", "name": "UtPXPhssYs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "zRSPmkqoXx", "name": "vhtEdaHnUz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 77, "email": "HztlyGCLjU", "name": "QCPaXPBVwB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "ihoKpZCndp", "name": "dcxYZwfZam" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 52, "email": "VOtfWMPAHr", "name": "jTACPpzuEX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "oFQcHFrHXB", "name": "ncPdVuyWfK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "qpfCJBmNIu", "name": "KJryPIyETc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 62, "email": "hJQrFJtehl", "name": "zQamOGqiqE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 23, "email": "UBDVMjiJPq", "name": "dHrQSZjhkj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 42, "email": "vHNlbbgynZ", "name": "JfGAbTYxaP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "wjCPvCNncI", "name": "HEEbqsYLsb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "TRkzCjHgsQ", "name": "ETXLSnMYmn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "AnuDesrEhw", "name": "iTXIIKwtDg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 0, "email": "mIfhAewgEL", "name": "LRRZutEDtw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "mMVKJUQyOw", "name": "sRJYobxqhp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "EJuAvWvWAI", "name": "lLYQpROCwQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "apMBnhcQiY", "name": "yoPvyyBOJg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "NlzpvzKRYh", "name": "EwDYFfmCPT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "jFOuMevwJK", "name": "HXYTskWVXm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 52, "email": "pLlkmVOpZF", "name": "SKVXgcDUbH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "ZrmzdRBAPq", "name": "dowuyVxTGc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "zTUxYLaIuk", "name": "YQgnEhsxFn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 19, "email": "lcAiaKvYiE", "name": "yIdBhPpBIw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "SGvUxxmLMJ", "name": "xuFOrOZPfm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 65, "email": "GvmwQHneGp", "name": "xDFdryQfMh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "RnjxAryZEI", "name": "TjdmgxoBPL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 26, "email": "EmEGiVLUYF", "name": "YHgfEJqVSD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "MwIlXbbvRR", "name": "NCkGQhOPmx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_simple_actions_1() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 22, "email": "DJZCyxYLzL", "name": "sGOZPymqtt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "zYgLJOtNcU", "name": "olFetRzgkh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "hbnzopjQWp", "name": "YaHAmtgEkd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 50, "email": "DRmbRPESbm", "name": "ZzVJvHtxfv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "gwAjqiuhFj", "name": "plxulbEhrD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "twKmvUiRBj", "name": "EsOKXRDQiS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "SyBdptYLHn", "name": "qwDHcdyiiD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "TFjlmxGVuP", "name": "rISkFtLgqE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "kYDgteuWNb", "name": "aGVXkzqtyI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "jTAiASdWKy", "name": "quxFcFbpfC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "haNbjrnAnN", "name": "clfHGVoTAK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "gcZLVipfik", "name": "DlxaevlWCY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "WxkuhJumwK", "name": "nnETeICFAY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "AneVPEgJIF", "name": "SpxJNtPYui" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "ggiPgUzyqn", "name": "fNCxzWvfkV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "FoXWGqYmQh", "name": "HAxCmlnwpk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 57, "email": "ATkzmofdnA", "name": "ApMcjjPQyv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 2, "email": "SzOSVZCfey", "name": "fGySNrPPCS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "SMgnfoJBfi", "name": "iqtrLuVmqc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 18, "email": "iTQhKsjvRM", "name": "oqxSZYXuqu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "GrYhDoFAmA", "name": "nFZmjPopEE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "wyGAVFLORR", "name": "ZbVSHUyYRi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 39, "email": "GqzwBBxJlQ", "name": "YIEMvdGCGq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "IjNLEqXenw", "name": "fNnwyZyqAC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "ezZiKvEqGE", "name": "KarreMqKgF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "KwDDuqDBvQ", "name": "STWTVNpoEr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "KngOPDKsdq", "name": "VdDhbwhKlM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 16, "email": "KNrKuOaGow", "name": "mTgJQRnJjH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "nbUAILQFEl", "name": "ShPmaCcUPp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "zwIzNCgPih", "name": "pQdcYbLMrw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 1, "email": "AkaHophCsm", "name": "nLRlOxCyha" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "IOufpfFReL", "name": "rRintBgRAi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 14, "email": "WwHzRNkItl", "name": "LBSzUYdsGA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "JApqXwMmeL", "name": "mZkPpWGuGz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 27, "email": "JmbuGZnAXk", "name": "NnumlnGRQC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "KVrPbmHyBn", "name": "CRirvNPHCj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 34, "email": "tWaKklsUFe", "name": "YXusjHrwLo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "jFUmesrriN", "name": "FzBsWIrtlq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "NEdwZnnBok", "name": "SzZzyEuQpz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 85, "email": "xQpgceAfHC", "name": "DnkFRpHWlv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "lyPuKviRSI", "name": "lvMebqskbg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 43, "email": "yYDkEXOqRz", "name": "ZCTUfhfSfr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "fUZewlquCY", "name": "MARvqagUTC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 44, "email": "JChBpqZrxZ", "name": "PLTijDMuyF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 49, "email": "FmCvQUYAnl", "name": "qVWyjXwcWI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 95, "email": "hywpyQaLTd", "name": "yDElKOdNMU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "ytkSAeQKMT", "name": "KSAgeMLnTT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 49, "email": "PbJJpBtJHc", "name": "JhfGhzyTWK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "AFDKkIblsf", "name": "aKJStiHEjQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 27, "email": "sGDZsohXuK", "name": "zMDhyXWCom" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "sgsusIKLhq", "name": "AsKnWGjdMX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 14, "email": "OahXxWasnu", "name": "wNWeHIDJEJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "NewYCLOeNp", "name": "sKIgqhJauc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 80, "email": "NjzQQMqYCk", "name": "DNQsUaaPPs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "DCYciaMhmt", "name": "VwrjuYJMjb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "OIPkgfWKaE", "name": "qkSSxmTjkn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 3, "email": "COBSSXoOmu", "name": "xulBZOWEqv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "cMlhiACMHt", "name": "ghXbmROrDK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "sXkRFhiuiU", "name": "BhDOvcvjSv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "aCbJjGLWZF", "name": "eeogGMSPmB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "LgAOxsZWBU", "name": "tuzhPrBExj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "iGvxXTuZdG", "name": "TLEQYcnHOz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 42, "email": "IWdvHAOkHm", "name": "KdTGfsCboV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "tAYtFjHVwE", "name": "wEWADxBFoH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "fsLMHvChPm", "name": "FfrRwqISzd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 23, "email": "NPHLVhtinP", "name": "NEuOIEvoZl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 16, "email": "IwgKhFzLJK", "name": "GIpbluNgZg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 4, "email": "jBYqYCaTeu", "name": "wyPOrrBlNb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "equQHlEOES", "name": "crmJGKHhfb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "rjNnbqJZaQ", "name": "kamSrZzmdT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "GzsGytVOGW", "name": "CDFLUlTJys" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 20, "email": "TEGFWCnUUW", "name": "sevqeyQoeG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "hGTajzLyqS", "name": "XmyKVhTXTJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 26, "email": "oCSkYvmiRr", "name": "qTJHOVxOpq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 19, "email": "hWTQYEoEBM", "name": "yzQgcraeTA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "vqfbyMAere", "name": "MMAUsFXxBm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "FjQUAupAIM", "name": "BNFBETaSIi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 65, "email": "JcyvkjUnQW", "name": "RYLBGxtkKw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 15, "email": "TqBBEGpuod", "name": "TMTMgIKdgE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 50, "email": "CQztuqiEME", "name": "rVmHXxAFzV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "qzXOZwnodi", "name": "tCwxwafUDq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 66, "email": "AoWdXqTGye", "name": "naDtqKbjzq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "tBfCUpNLfg", "name": "DsIuTqQEnx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 14, "email": "gBAjlchKes", "name": "TNalDXdniI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "lOpLSPnzNI", "name": "NWSCzIbdFj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 88, "email": "lyLXaoXTOm", "name": "jOxbtnjtCL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 17, "email": "blamAKwdYb", "name": "PxxyCmbyfk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 69, "email": "rqbYJZYydv", "name": "Dnmjxdubnd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "wapILlclYi", "name": "bTIHMXMYfY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "HAQPAqQubI", "name": "DxvTfxxbbS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 98, "email": "GcvQEKAwlk", "name": "vqJETfKmZe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "WoiDsRzHBh", "name": "oUAGkKAYAA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "cLHecOFjiL", "name": "PvrfaEqmyb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "DOgMdEFAIP", "name": "nWuqeuFYTG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "KCtlSaIvOJ", "name": "HONYMAavAA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "bmPhVDZvLO", "name": "SuwrHYQKEi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 69, "email": "GtaolVmeBs", "name": "keTCeLsQkN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "FgyzYDSTPa", "name": "BibPhPIfic" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "AoMoJPzmZO", "name": "XiqauzPnkT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "tuzGYydZAh", "name": "vKTmvybtQG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "AxzcWUTXYA", "name": "ZESIosbIHN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "pCahwKtFWX", "name": "bgEqiUFkVO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "AmOkoykoTe", "name": "OeLOfmNUSZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 74, "email": "cuWXupbKlJ", "name": "lxnVwLgojb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 36, "email": "EqiqTytnnU", "name": "OPSNJhDlxH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 38, "email": "TGiyTonxma", "name": "uzxZbYBWbm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "bjrVARXYcp", "name": "AaCNahRWmz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 44, "email": "NeYuiXXzHX", "name": "BdvGYEJBHc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "AhjaehLCcm", "name": "HEtOJDqhUy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "gKZKbLpnwV", "name": "jBpDMGWhPQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "ekwrSkTzEa", "name": "eCsziBivvw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "FQifZBEVCh", "name": "GDkTlFdmuC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 6, "email": "vgnkgihUWU", "name": "XWxkaLLcLR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "QjUtkViGnZ", "name": "tJrSRwHRrY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "acbYVgKSfn", "name": "ahzDUnixar" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 16, "email": "czRUnBKpld", "name": "NeQDInuHjT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 18, "email": "FsFztqHgrz", "name": "ajLYtNtrVy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "TAvCylrYQq", "name": "xwhKvIlMBo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "lvkOlkQIrx", "name": "rsDYzCCrOv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "ZCyGaPjjxG", "name": "jEwvKttCeX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "VHTljgziVS", "name": "DJFSaejbQf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "KllOlTwnuj", "name": "AluVasOigE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 3, "email": "nxEpHmAFJu", "name": "fSWXTgPNLc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "xXRsAhHLmr", "name": "ucGCLWIVCx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "uzeIYXplXd", "name": "EUtRbSjuIL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 54, "email": "XubIVnknJZ", "name": "sbeVtdWrXx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "RkTXEqAjPi", "name": "vybkzdrUMa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "pTmpWUJiEX", "name": "TKTEJBsrFm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 40, "email": "eHQVqdlwiO", "name": "HrlbliNnDW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "fWdVOnhGqF", "name": "utBSuvDenp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "TkRPRRnxSw", "name": "bMpFbShchq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 77, "email": "ChbPwymNHe", "name": "dJkMOYXSxa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 31, "email": "VASxEAWuJG", "name": "ZcvgRkCEOC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 87, "email": "GANmCuNFmH", "name": "YxpCgSGIjB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "SJHEbTloOJ", "name": "YMYvdpsXti" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 32, "email": "gVHIENDVLu", "name": "IiWeLdDOAr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "YrCRyjQJpW", "name": "PlgIvypAFA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "EAKslbUygj", "name": "AkdvTaGvYR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "zfcuNdGKud", "name": "kEReRMnKyB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "PBLJxQfLwm", "name": "pAjLjaMSJi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "yYlRAPOpNA", "name": "dDeLpppvPI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 94, "email": "sRWdazJTHm", "name": "HBOpXkyHOo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "ZqoQXwWFmp", "name": "LlVoEnUFeS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 75, "email": "xipVdlVLuf", "name": "cRjTMqhuet" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "uEoThqaQXj", "name": "TPTBLhhMXG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "dZOONGovlP", "name": "nCOUleZFbX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "bFNxUsEVJG", "name": "fKLMrghzgM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "EaTPuBeogE", "name": "eLxYiGgVJh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 85, "email": "FeHapyDCxA", "name": "HDjaGGkoEP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 97, "email": "RfLYzUoeTG", "name": "vGINgcfGsk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "roWmkpojjv", "name": "wHGOjLMCbH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "eWmioISHAV", "name": "gdnlGABodB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 49, "email": "GwNtHBrFsp", "name": "xdxISyRqSD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 28, "email": "ejLqQbnVrW", "name": "xXHEymYhti" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 11, "email": "tAuTkoUuan", "name": "oZiUvdwGgf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "SPuJxhnNkq", "name": "MvXUVJjimK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 56, "email": "JalQpuAkor", "name": "VXTWekoHWy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 24, "email": "kzgyUoKgFA", "name": "HtfKyvmkWL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "gxDEUmEbFK", "name": "PvjRqLnVPt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "vugRiQYTnU", "name": "XppFajRPpp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "hQRQjoMtMQ", "name": "paOygbfWVF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 87, "email": "aCOBmTOKxD", "name": "clDNLkYsXv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 55, "email": "grosmzAotF", "name": "miOioKRhpE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 79, "email": "YTXfvgFTJi", "name": "udKFeJgKoO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 81, "email": "LhRWtDEbKF", "name": "FKBQMREGMz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "inOeyNjBhc", "name": "gnuIJYYtKJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 62, "email": "PCXsnbVfSJ", "name": "dVdFfkyBIH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 40, "email": "wUhWtZpxku", "name": "HdZHlpwJlV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "fbuzCTJCzQ", "name": "yJgUkCTqlK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 59, "email": "xRoiFmnLUU", "name": "aJjOuieRVQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 80, "email": "JfJXghsVOb", "name": "wkEwXFDzGK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 99, "email": "WoAaRStrTW", "name": "mmNelqspjQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 68, "email": "nSkcFdtZqo", "name": "hibyKDxjYJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 44, "email": "XlYgjHKChF", "name": "KCzGPNdCzI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 99, "email": "OqBJBQacKH", "name": "RHqRLPBqRZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 19, "email": "TrjnsukUbR", "name": "AVgpYjXwiL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "tkUjcXoyNw", "name": "uDTOkEPSht" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "VhIuyHbCsK", "name": "rnhwWgiwdr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 9, "email": "UmoYMRFKeI", "name": "ObAKPFTgOD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 12, "email": "nqikdledor", "name": "JJnqRMziuL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "kFXhliPVTx", "name": "UoiKNEzlPd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "vWbDwyDSzv", "name": "yUZtEuowJP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "jKHMShhkTu", "name": "jQjaVpZaoM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 18, "email": "dZTmbCVida", "name": "gfZAVmJFAf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 96, "email": "rzYiHiDjiU", "name": "jglqXahavb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 41, "email": "bkAwfnHSlm", "name": "DfHkZNWjRv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 33, "email": "eJvuUNRErA", "name": "rmYDcYBnCr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 22, "email": "rUwWHSiNbF", "name": "CzHDYnrlxv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 31, "email": "aKTFrParZh", "name": "rdhyQZmLkS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 45, "email": "ladxOSaoEY", "name": "iEZPSAnZBt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 26, "email": "llVPnUyHWW", "name": "VavixbIuZx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "LgcyPHZRrt", "name": "fLrXioinBe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 57, "email": "NwfSJHqNlW", "name": "tRrBAZqXEY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 84, "email": "bBqmrkcAbi", "name": "vGVBTNmKKO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 98, "email": "kbbqGCLwhJ", "name": "SDlsfOZgkE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 69, "email": "gcUqqPBTXp", "name": "nvRMVsHHNK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 17, "email": "MynOqCIeKG", "name": "gNNQrWwcyl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 73, "email": "MBdDSLFaHD", "name": "qbYSQrsKGy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 13, "email": "pVfIVBCNOx", "name": "oTEsFERCCH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "NjBeOIGhqL", "name": "WIxjFZJAVQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 5, "email": "uwPqbLclnS", "name": "qETwSeAbmk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "lNzkmOqFIp", "name": "sBSEQKNWRQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "LImHbWKvRA", "name": "IkrfxFNIwK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 46, "email": "MUYGPXLROE", "name": "lugAZrztYr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 77, "email": "VtvuadaItp", "name": "OaqJfKrYpy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "HFJsYfGLFc", "name": "mhdIqtxTSv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "mxOwVoultw", "name": "DEukbLRDMJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 82, "email": "aaqKlmsfym", "name": "VQazZwpmAR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 90, "email": "QPYBXSsMGB", "name": "cnlbvBEUmV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 60, "email": "JhbXHZdCFx", "name": "oxAWRhlgTG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 78, "email": "qfzEwfevHa", "name": "RWemHOEbDH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "WHAsEnwKzQ", "name": "ywzjFggoNm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "jXCjKtliwQ", "name": "yKtvRFMhGJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 67, "email": "VhwzFVlIxB", "name": "ewQxZeyFDl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 99, "email": "MMhyUHetlX", "name": "aLuTNUFUzw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "gFIuvAEWjo", "name": "jOtFZSRBPc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 64, "email": "LsiaYmMxSe", "name": "TzGpreyrUs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 86, "email": "qZokCuBPKU", "name": "WbJlUXesXl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "KFakYVNKOH", "name": "OXxLDsYOIj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 71, "email": "KPDquWrKQY", "name": "ZNRlKfQbkW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 0, "email": "dafrMeJbEY", "name": "npjpKJQqmg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "yHHLoiDJCs", "name": "VrLaLaivSW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 84, "email": "atUJJAfnDd", "name": "LBVJFtyUds" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "sYghiCOHAf", "name": "cQkCixqhFq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 8, "email": "tXRigJtZNN", "name": "RLvCOOwYWZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 26, "email": "YMuLsyWrfg", "name": "lBLTQjlUsw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 83, "email": "YnRMkxuOpe", "name": "cojrUreeMd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 70, "email": "RXECwXutzt", "name": "gGCahwJeOM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 21, "email": "IJxFbIwkHz", "name": "HABFJVprYY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 59, "email": "dJkoayaRjk", "name": "oxbuugKdkg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 69, "email": "QjBBSCkHcX", "name": "qhAxIwVLoX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 51, "email": "ymbhlTEkwS", "name": "fEUqSDvzqW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 89, "email": "IvtPfOHpcs", "name": "HQReLarVBt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 76, "email": "aXIXNNdSsA", "name": "gBRBEjHqjN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 72, "email": "hCGsUUjLPA", "name": "YaDVjvbtGL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 68, "email": "qsQYGnalhI", "name": "fLXvxzNfzt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 61, "email": "kkTtAjmUYV", "name": "FRjZxJRBXh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 17, "email": "rbPCyvBAPb", "name": "cosFslITjJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 29, "email": "BlcLvDeOtD", "name": "oxeaEqGdao" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "aRotQTcOhb", "name": "hdQHytkoGa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 47, "email": "sqirujUjAr", "name": "GPJhoLYqdV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 63, "email": "MTJBntSagk", "name": "axUMvdVbHT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 58, "email": "bKrjNgZsPG", "name": "GVbbSoopnS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 92, "email": "zkXLzVwFFl", "name": "mCPsPTWhQe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 6, "email": "IiwjPsqbGi", "name": "bjULmekdSj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 48, "email": "tYrcuNcVVS", "name": "QILjkUlQbK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 35, "email": "RJksRCpNQd", "name": "PMLbIeFCHZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 59, "email": "iMgokEXtfa", "name": "wLIoJklCJv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 45, "email": "wIMxzYRkJY", "name": "XWjAQBXEcY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 38, "email": "qeFZMlQuNM", "name": "rhvkoBGIMh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_simple_actions_2() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 39, "email": "BMFnDHevKV", "name": "NpeFdGBszl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 93, "email": "wUMNdsISff", "name": "jJKWcMlwni" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "age": 7, "email": "IAwxhAfTMt", "name": "rGYePBrFeV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 76, "email": "MaoapjjXrc", "name": "DsyxKmVRrw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "hgDCMKXgsn", "name": "nqKWiAPZSS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 67, "email": "TYdGiVpxux", "name": "POWtztasJm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 17, "email": "doVkTRyALR", "name": "PDWrzXLweM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "gqaumCYAPJ", "name": "CPfGBYictO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 19, "email": "DnUcuENgMq", "name": "SdHSwYYXtA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "UNFwbkUKJk", "name": "RMWxgrvzrx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "LvUKOqWNlE", "name": "TEdRelCaHG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "rauNJPwfSq", "name": "PDVdhYzYYB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "YbVNrdjUUo", "name": "iIZWySrvkp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "JfchNUvylV", "name": "TlPbGZKdoe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 96, "email": "mvUbGEsANQ", "name": "JeKvQuKrAt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 53, "email": "BwyHtWeqUl", "name": "dhXpjcposm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 86, "email": "BepYRlYBtr", "name": "yXtIsffcpX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "juBfszTmGn", "name": "UTQOqDsQLu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "hswbNJZaBA", "name": "hvXomHiUpF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "lGpupaYRUH", "name": "yKkEaPqrNb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "rSPOGfeKrg", "name": "ZdBmYZsWEQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 84, "email": "wZIjuXDpsN", "name": "vHchKPmsVB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 48, "email": "SaesaszWWN", "name": "HenqFbOgdg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 84, "email": "iFmcfseWOO", "name": "NppyRpXjXZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 14, "email": "JcOALLTXIZ", "name": "cNHzVPdIXU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 83, "email": "LWMRGsCYJC", "name": "wlkCFvZvnx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 30, "email": "sWmnmQByjj", "name": "BbzNSCuatA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "BSuCyqDzLp", "name": "ThAzqZoRSK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 86, "email": "JwMukqSdls", "name": "arNtrKDwQp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "JrTFxHkwJi", "name": "DfxRUyhoHu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 24, "email": "xiFxbrBrsW", "name": "FPTdCaFZCc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 57, "email": "gQDeveMuvh", "name": "HcosDmnQur" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "ciFfzApwZB", "name": "zixQVHkNnj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "VuebgNwXFZ", "name": "lalMqBNqbl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 15, "email": "bhtHKWXMie", "name": "raBAnKyEXF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "NlALyuNNnS", "name": "gXoQSxuvcO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 85, "email": "KCahFSThTf", "name": "SytwCrTYXe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 55, "email": "ffLeyRGQcF", "name": "JGexUowLhP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 30, "email": "qspEBdlKBo", "name": "UJSUCxqvfj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 52, "email": "mGDEjDoccN", "name": "FTgZWCbGVS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "WhdrwvXQcT", "name": "qkHarXPthl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "ELGaVlMTVD", "name": "aPlmIIOLrY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 6, "email": "nyiATgYUeV", "name": "UIdRgeXUTj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 17, "email": "sLlDVuoKbb", "name": "pNlxPTHtgg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 52, "email": "gZXbbvGFbo", "name": "FUZWuBGEAv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 34, "email": "UhYrZDlNyb", "name": "SsrybKAnZs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "wzxACaFkcS", "name": "cOCpIsyKSV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 17, "email": "xLgSlWbnqb", "name": "QRpJwFpkQR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "FhsMwmjXMD", "name": "SeoGiPiwwg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "AqjOKflPSj", "name": "VKUYUPxqLB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "qRfEcgZgym", "name": "MNrnIGUnBH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "XmpzDChajV", "name": "hCxWqDWBfT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "blkyPbdtsF", "name": "wrXoToZiXe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "KHOcqsGreH", "name": "YKDrWGNoJW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 44, "email": "TDOVxLYXrO", "name": "MssjIFELGq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "zouquEJFGX", "name": "WvyVAeruqL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "MPlTzvBCVj", "name": "POeZdLAFmP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "vuJTvytAch", "name": "torjYYZUVG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 15, "email": "NMvYMaxxDg", "name": "pkmyjAJjhQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "DzyudUXhjk", "name": "yReSPTDokD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 91, "email": "bQCXXTZuAe", "name": "fjTyKXKeKv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "kmROoILyNn", "name": "qwkPsfYFXD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "ErqCIdtOeL", "name": "hZkzIHaeWD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 54, "email": "JarACvhksn", "name": "PPgXJltSoX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 26, "email": "aYOTSRuTRv", "name": "YUpuLABqYg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "bVPTJKXaNe", "name": "QqbhMOynnL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "voPwWLlWqP", "name": "EJZGtrUFXn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "jiSZJXzONa", "name": "FnVjAaJwSf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 19, "email": "FSfvljotNs", "name": "dvuadnOPOj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "rhHpZNgXmV", "name": "ZOvWTGktWW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 12, "email": "bWFehToFEX", "name": "gcZSpmSvwT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "VOpemwCtyP", "name": "VbUegvDtKK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 20, "email": "rFWqUgZLzX", "name": "mRTnZmJXAm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "AtDsfbakfA", "name": "egQCTeMzkp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "KDRscXkAGG", "name": "HUOAuVXASC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "EuveoQvHhc", "name": "KQvuJTTBbD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 44, "email": "rvDZDIamiG", "name": "HzfeYYjlkC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "smlfcOIZKz", "name": "nAaLvXXZKj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 80, "email": "NJzZxcsMJr", "name": "moWyLHjqec" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "dSteVRKjEC", "name": "xyIPPhXPmN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "yHXdQYllxe", "name": "EkNoZCrqKg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "PTVYReiYMm", "name": "ClopdwLvov" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "gMXjifMgMF", "name": "DgUfGwTpCr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "sSdrHRhurS", "name": "bmjxUXzovV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "SGmlWbLSvl", "name": "tzujZswGkx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "ezYMXJwURo", "name": "qQzBzuyMOI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "jnZszBEsbr", "name": "YqrcuYIgcF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "gegUcvpiys", "name": "wVzDefeTxI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "cqaYNAFRhv", "name": "rlknOiFeXg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "wUcufFcSPQ", "name": "dYrElcvHMo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "jbbqpPwOCy", "name": "aqVttDwePO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "eOKEQQwxsq", "name": "xqgfooITdn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 52, "email": "WVWzFEcYWT", "name": "gYTaLZmMpN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "nBPUNucHvY", "name": "UgaPAvpYGK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "hMhJPfWFfD", "name": "EsDGdJZhZt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "yiwxtcdWSR", "name": "DoPZcEtfAA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 94, "email": "ktophhVWyI", "name": "WwmyftrtDA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 92, "email": "YExWNXbkai", "name": "OAyZOhkrqO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "oqXAdGkQtF", "name": "nptKmNulPl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 24, "email": "rUJOphjSJu", "name": "XKGonVfBxA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "YjtqEqOXtW", "name": "qaiZkyrEOZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 76, "email": "BGUHolHrtn", "name": "NykcafHrTG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "BOKvfQitwy", "name": "TAlJYeiXAZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "DVeozSEWkt", "name": "OtqoTFpHXs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "bSAWOagcFh", "name": "SKrZhPJYOh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 18, "email": "cxbihQMEEs", "name": "lFSBxlwoaf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "ajtlwVpJJb", "name": "xSwMZWZiTy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "jMELhRhWru", "name": "AspnPZGLBw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "SmTpelHaRq", "name": "NGFgZtGSMB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 67, "email": "iwHQUtsDdu", "name": "mLlDxlEcWS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 89, "email": "UCRbVyNZCt", "name": "crdvAfbNhx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "OAfFQNyYWM", "name": "iuBhfdyDtE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 56, "email": "WlThxdcmLH", "name": "JicOokdxEt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 91, "email": "kWRURiLKNv", "name": "nkirGGBDVG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "zVSqmIaGme", "name": "namxauwBjs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 0, "email": "dhLXmKLLuO", "name": "kDhjjMMCag" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "JQgJraEpip", "name": "QWutzyMpgX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "btoxVuPyWs", "name": "FAIKoUHfpD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "HSgNklCXHT", "name": "DkFDTuKfYp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "EzJbuJRhlU", "name": "sNsRwbryCe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "TqXefRcmOK", "name": "jyDBZvJYGt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "OMyvIjgUXo", "name": "HirIOgxFxr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 26, "email": "zTukzCOeAA", "name": "nAnYpMMNNZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 88, "email": "bNGgtdTehn", "name": "FzJGbmbgnV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "kXzjvlrPbC", "name": "YVDLaKZGHr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "ldaotCBUkd", "name": "WiiJiDGAgT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 17, "email": "kTFSvVovdy", "name": "LkybRYuULV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 9, "email": "wvyzxyumAV", "name": "iKaNuilpAJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "oxbGXevfUz", "name": "tuufQnjFQG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 77, "email": "cInaIAYlbR", "name": "ZxFtQcdRUa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "WbekfMjLlt", "name": "rDIPWNBLMf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "rnODVgZwBt", "name": "qvfjDUlfAu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 42, "email": "JNLTBGEcaP", "name": "IcNxoIRtcf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "unneNTHWiX", "name": "dynTltyHIX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 77, "email": "PxilvqKOjU", "name": "bJqvMXZLvT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "TfsIOlZfXU", "name": "mPPVQUiOri" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "hapzdZMykG", "name": "xMyGIAeDON" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "dOfuFmKgPj", "name": "gbffjDnKfb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 15, "email": "SwxkLfFpxq", "name": "JLhxoGfsCN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 57, "email": "bUcqjVRqgP", "name": "riYgTLCrnJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "eCoPVwSAkp", "name": "lugsvngvJG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 54, "email": "hFbmarJogj", "name": "wilpULOAnt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "UfHLtloHoc", "name": "JrOaUGYaKz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "rTrSIUvraV", "name": "gCJMUQmexK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 73, "email": "FNbQHtWlMz", "name": "xEoVqwNOIe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "sZcUfyoUto", "name": "dvmyDAXSvh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "lmjaDTgQpy", "name": "MieNvzqldE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 18, "email": "WktYXnsHoG", "name": "HPjQxdYDty" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "eJpsiVkBqI", "name": "gHmyitKShE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 83, "email": "eGXSfmBLVG", "name": "CQZDTanyld" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 84, "email": "ejNfMIIXwr", "name": "ZmTNGLLbFB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "PjXNbjfPLt", "name": "zmwjbWuULr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 53, "email": "kCYtEaADoo", "name": "aPCUxurPAw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 68, "email": "ELKDOOUFik", "name": "qHJduwqNlu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "KwuOemTTFr", "name": "gRtvdNVuQf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "bYcHIirfCO", "name": "ishAQclVPy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "PVOTcJxmKi", "name": "gpFwIfNqbZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "iVZVEbRmfT", "name": "umHjzDDvaI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "hZLXGJqLsM", "name": "kPFBGTuTWF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "pxQMMErjHD", "name": "uADdyMFSmW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "LqQfzJwbEb", "name": "NYMKkKazQh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "QDQBMYNqYK", "name": "THcgaMpAUZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "snLnNKlJZf", "name": "QJkzUIVwHv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "nXiYxxtRyD", "name": "pHGmiZgVpu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 30, "email": "iRoBaRFmXj", "name": "pJndLKLEFy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "cEctqaKLnD", "name": "bXMvBXgfCA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "KISrzwKqQO", "name": "YpSiIgvOMm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 56, "email": "iGHUlBrkpF", "name": "opHEigxJyJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 89, "email": "eQNebzawwy", "name": "IyDOVtbhyD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "FmmpxLIeyC", "name": "OkbKrBwyVg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 88, "email": "kiCmnpAvdb", "name": "GeBxoFtqQf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "PmHLUThqDd", "name": "AsAJgUGURy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "jSyjKyUfpH", "name": "FOxULtcDId" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "oIxzvwKcci", "name": "EUEJYzlgcb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "vayZbSghEI", "name": "gfFizEmHbk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "dnphvBdJLP", "name": "xZFAkUYbxs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "iVImrfIZIt", "name": "ayFpgecVab" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "ZjxwNsVVds", "name": "ehYwXfvjVD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "awhitMGUQV", "name": "CBGvPYULSh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 58, "email": "yqEZKtIkPa", "name": "REaUVKDphE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 53, "email": "IulFTTcDqS", "name": "nylcDCKavw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 54, "email": "dMsGBERnYo", "name": "lVaimLeEfK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 17, "email": "SPDFrqsROM", "name": "xnYIHojTUR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "gWGzhjfpkv", "name": "CcEkLZkwbh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 67, "email": "uURUOAlHLX", "name": "IuTLsQwtkQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "zLcuKnFnwz", "name": "NkICaJGCZm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "oDEAPJsVZv", "name": "mGeOewlPzj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "gYeIUgavqZ", "name": "ojmcUxjPLc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "UCcnpKLOIp", "name": "aFensoNzBG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 22, "email": "YbixOInbXs", "name": "pLsaIESFbR" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "rnjXAlwhXC", "name": "hQALnFpiyx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 80, "email": "LKwThvFZjh", "name": "NDcGKXdgFu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 40, "email": "jQbweMhOfk", "name": "aRiTEhumjo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 45, "email": "WVlZzIBoie", "name": "deyophSbty" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 44, "email": "HIkHmSesBl", "name": "YqeItsGeBW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "KhccEjLFNb", "name": "qYiPEFWHoB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 7, "email": "NtSDaVWwam", "name": "LGpSKlkfhN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 37, "email": "iAStqXHguw", "name": "mjOHWYxzEv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 42, "email": "MDAgUrdslJ", "name": "OxZdYKCkZw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 95, "email": "sQSQpGvziz", "name": "pFUuxNDwcB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 80, "email": "pKACDwEyRr", "name": "qutWBsaoRl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "sMoMjsbwnG", "name": "ybrjuRvfUx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "YdgUaLSjBO", "name": "UWqbUKUHbO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "PmNrHtaPGk", "name": "pZapzOqVQB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "attWlHTrGJ", "name": "VdPAlAAbff" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 48, "email": "RcVeMMrhIl", "name": "hhRzGdXntn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 75, "email": "tUZsvDFHmm", "name": "DBwtKqNhAa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "qsgWuNxQep", "name": "goTHndyNkW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 40, "email": "pINvmtBihG", "name": "qSebhwdmuk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 87, "email": "AZoZcbnpQL", "name": "WkTvvmGBPt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "cJtYiMiuiT", "name": "nUbUaCDLOQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "RjuZniuGJy", "name": "ogHUiDYEUe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "YSBBuxiUKB", "name": "wPGujjFfHx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 6, "email": "gEpFOeqYAo", "name": "kInNAoleTm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 79, "email": "vJYZPiZxDs", "name": "IETuDVWrHn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "CEVfsjSEpu", "name": "ZLWyHbWsaU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 9, "email": "UxjyHqTPUt", "name": "dbxpIEEXFv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 86, "email": "EHIjyvBjip", "name": "ZThrYBcIxG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 37, "email": "acJoaNobXW", "name": "AKAdyIZHkF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "ftcgSKekVc", "name": "eOJBabNFtM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 3, "email": "NmoUCNGYuS", "name": "jVghTkGJQf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 87, "email": "SftiVhiVJx", "name": "znIjelPArI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "MANHgqvVvr", "name": "vziCPQCCDa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "PThRPHKhgj", "name": "sCWmjiGHjc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 91, "email": "UlYiAqJHZx", "name": "xxlzGlngYu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "sYCxSUKhwK", "name": "MkriuRpTBF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "AAiCYkmuNy", "name": "YRknXprAJY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 84, "email": "VPTquckhUQ", "name": "HHSfHIWqHF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 38, "email": "WWMoqkaGFB", "name": "TfulNyYvPM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "WBMxOPLIlq", "name": "wYBjkSwKOq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "vVSCzurPHr", "name": "TcCjWGXHyE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 77, "email": "IXKCSzQQSu", "name": "tMWptRawbu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 36, "email": "vLlbvAnWME", "name": "GeKpSWDIbp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 58, "email": "hjABvGPjpy", "name": "BYTHulMOUX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 26, "email": "OLgxnRRgRh", "name": "TNoPcbiOzc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 85, "email": "oSLNLXkhYu", "name": "RTvuDXdCqr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 68, "email": "WQeEOxPZOa", "name": "OMMpjVQEvT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 54, "email": "jjkdcaQQSp", "name": "dbVylZjSwV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "GzhyfLcuVU", "name": "FFovDVGmkk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "wzSRhguoCU", "name": "BMMrADISRQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "qqIbOJUiiy", "name": "pCCIFntvbg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 22, "email": "ayJnafKKcT", "name": "ccEWtDGTZS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "OgBBGNOXYA", "name": "mobaXXRXhM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "WVjmbSUipV", "name": "NxHHTYhUFI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "fEFLnGINRf", "name": "pMDrpkDTRQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "FLTrQRKRQG", "name": "dVtUxwLelw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 80, "email": "ssZzyPYzRr", "name": "IpSLYvLuaz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 68, "email": "aagNdSqETt", "name": "ttpxbIsVvJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 16, "email": "SAfasJHMFj", "name": "duXwrDYoNG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "kKHTXXjtAV", "name": "wTFbQhGmXM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_simple_actions_3() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 91, "email": "DUINNVbQTf", "name": "olCJaABBUk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 0, "email": "WJnOdHyMqu", "name": "qKelHxkPCZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 67, "email": "ESjySQdZxx", "name": "aBWbhluVgm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "QaLTyTvFGy", "name": "cDiwuVsPqz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 12, "email": "exChnPIuPe", "name": "tkAqIZJanV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 70, "email": "WlAnTKBXys", "name": "ZdAqRZByGj" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "rOERmBOAzf", "name": "heoprQqADh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 3, "email": "SuVWPnoonN", "name": "KArrQisFup" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "sijICAyhOg", "name": "EffNvhAuLt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "lxbTPghsLW", "name": "NbPrxdkBLi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "mDBnwljYnI", "name": "RJkpbHVSge" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "ddAchGuqlP", "name": "OmsVwlZfgq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "KNEzNhbRbT", "name": "qdBneWdjFd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 53, "email": "JrxEgbTXNT", "name": "faELLQafla" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 89, "email": "HwAoPqZgKP", "name": "MfjLnZdIKV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "kYYWkrSaNL", "name": "oRfzzrQPeX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "CepUQJPxmA", "name": "AcuwEOrLgI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "lWjTqLERvm", "name": "jhGlpOwkkr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "qckWQAGSCf", "name": "LuDTcSURnc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "BrscIJzZOH", "name": "JNUDLmrGWh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 56, "email": "dIWEczqKpP", "name": "QwmxZbUZDe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "vlQYSxwIbW", "name": "iIdmnfkIUp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "ulcomoYPyt", "name": "vIkSgYQkDP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 48, "email": "RRqTeYCeCu", "name": "vCAcFcNknJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "yXrTRxkyRL", "name": "JgrriRNdgI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "mPdRnGTKKL", "name": "LOzJQTGlOF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 34, "email": "seBxxipTIm", "name": "uaGVdnYrWS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 24, "email": "MwvAxFESpM", "name": "mdcolQRpjI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "RRVTtLUxkh", "name": "wTWGIEpeKd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 56, "email": "lRdQSzJCAo", "name": "dMvyfRlVZE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "anncOZwNnW", "name": "PVnIbDYMDx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 67, "email": "AdcovcdRDi", "name": "HYnHwqbFIS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "nlRVdpIZtN", "name": "SLRaebjInm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 20, "email": "FQjVLvHOkC", "name": "qNwwkDJUOC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 44, "email": "yzlkuvUCaC", "name": "lYVNqgsSSH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 87, "email": "AcoQDryUFZ", "name": "JABJwaWTrI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "ppgItPQROU", "name": "pTTkzjKAWD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 55, "email": "IGvuRhZhwt", "name": "PvNzkOzxcv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 19, "email": "nlSYWAEHyN", "name": "AIXoDurVYS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "wUYkGgCBAx", "name": "FqkCZfGwUW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "OaCJSVQIFY", "name": "EYzcNyYhRt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "wTkDveWTdM", "name": "XICfMpwhEp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "JXLuoZYYFQ", "name": "kZgoIywVKO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "deEtNIIDCD", "name": "ZHBkfNJjMI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 42, "email": "yCnGnERDuZ", "name": "pxNBOfIDPv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 56, "email": "CfIhvoUAwr", "name": "OhvqXpSSFZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "rlbHpmFauK", "name": "WAnNHqTOZT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "JXbmCPZkoW", "name": "PuXGlXiwlL" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 4, "email": "THXzdSOCQN", "name": "gFnVqjTbfo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "lwiilzHjET", "name": "BSulVjPgMZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "agouosCegb", "name": "vHaguYtNvs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 79, "email": "pzHPVqECvX", "name": "KAGPVgwLMm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 42, "email": "NNciLMHWyZ", "name": "HsrFoqUIfX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "HUWJEWHtpG", "name": "yqjwBCesVF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "znajQrHFOA", "name": "ZUtYsBlpyH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "luieDTjMZR", "name": "sVLdirVqWA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 6, "email": "yFcYUBYROG", "name": "DxpDyFvRic" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 73, "email": "WYiTKcEmQu", "name": "meYfGjVGoi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "hyBHeQuqze", "name": "MicJikSwjC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 86, "email": "gnsHnaTyMF", "name": "kKRSFcZldt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 55, "email": "omxoijCsyx", "name": "EDWWnDRoFY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "LCCqLSJRlb", "name": "XGTzASIPrv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 94, "email": "XsUDvPSLxi", "name": "ApFPFGDJMa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 96, "email": "mqksKVfGrB", "name": "SvqPMsOWeT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "gRkgWlMCGZ", "name": "mkcQSapVWB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "wwXzBgXRNT", "name": "AfsqAtQOzF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "dRqtgdGoGc", "name": "lrtMCaJOxu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 74, "email": "UlXmvYHURj", "name": "yjyVmZLJyG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "LFrkHvJkFK", "name": "CynjONrmqp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "DQwvKjEPVF", "name": "dcEbYMOkUy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 55, "email": "hBgAHVjZpF", "name": "rkKvdTzVGS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 45, "email": "TuriFqChKF", "name": "SYmihIDMHG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "LjIqWfvbLm", "name": "oUWGRmMUgu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "BTnSaMyUOU", "name": "TeLDxIGldB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 58, "email": "VKAFhVfjZj", "name": "UWQOVKoqcN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "YCLsaQPvuI", "name": "OcmfaWfCsJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "QCpJKftfIx", "name": "sEBMUPBIfN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "FTPZmpDRNF", "name": "bftZMDvjrp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 77, "email": "FgLBLpemRM", "name": "VqocpADNeG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 19, "email": "TlgLJJMqbm", "name": "laqLxpehkm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "dCuDBActeJ", "name": "eYuEIKyxQW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 9, "email": "NpcXoTJjmd", "name": "vFdcIXxjgX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 62, "email": "czbrrfvYtx", "name": "OyIEFfKrTA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 76, "email": "JLXRQdYbpA", "name": "KKcyQKzfFF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "EyVDOFCZcB", "name": "gcvAFZcnFH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "OJCtxykIRm", "name": "fxXlmdVzCr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "moXVgAppaZ", "name": "SMTNspwuxJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 34, "email": "YTEXelhQLT", "name": "XGZBpJyvbU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "rLZVUoBDht", "name": "FxuKaijyDX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 87, "email": "ntySPZSCnb", "name": "ocqRoFhiTa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "KkctWhqSmN", "name": "VFvtvVpFdO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 95, "email": "zKhqEaCvrE", "name": "cHvIvjSqju" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "GfRiOSGtwk", "name": "dQEFlOLCes" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "oJrOncltSw", "name": "hqngAGvZbs" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "nrgdOmZFpK", "name": "OWEvbLzrFC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 13, "email": "sSJXzucaEP", "name": "KNkhowhoFX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 40, "email": "GzUQZXrkNJ", "name": "iGRyBuXoyq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "xkDBZPJJhC", "name": "VlLPzOUCFM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 37, "email": "MfmDEQHPUp", "name": "KzVYaQrYta" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 76, "email": "srgspzgUTj", "name": "eJWjSNFmOe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 86, "email": "qDkQGttvgR", "name": "kqJkuvHcGG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 76, "email": "ZExCpSPsYQ", "name": "edlyoFRvOa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 20, "email": "dmahQUGxJI", "name": "BGOAwRNJpx" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 92, "email": "kPkoErpDhl", "name": "MWqellrVbp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 95, "email": "MEiIWYXeHj", "name": "QpYhAoMCmk" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 20, "email": "WPdPKzpROO", "name": "hezvNYsRdA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 24, "email": "FkfeswYAUB", "name": "BSBKysoQgX" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 42, "email": "ZSSVTzsUNa", "name": "FAUVFbDzlY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "ygtjmmGnss", "name": "aaHYVfrAta" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "bdwTbTwwSk", "name": "fkcKZRqkIG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 77, "email": "AituJByofJ", "name": "ZnupWAbbzq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "qZgIaOCnlp", "name": "sGdFnJMayJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 29, "email": "RjXwQBvWEw", "name": "JiMUruktcI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 50, "email": "NhAwisYzOU", "name": "hsQLYlPlRW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 88, "email": "dXVbbBGFdo", "name": "DoadLeVMAn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "DJhHHLjZBu", "name": "vWhsTlkVjw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 37, "email": "rpQAnJDwHS", "name": "nxhplHVPPH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "oAczxCHxnG", "name": "kBApmNpbda" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 80, "email": "vIvPeEfvds", "name": "yWpbUhmRgl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "IDDOIoNQQJ", "name": "WmyFNArhIY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "hLZWwUKaPk", "name": "CLpfZsNUhE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 82, "email": "RoJcTFcxYi", "name": "FGvNjHXvYQ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 40, "email": "URmyOPouac", "name": "sTCrWtsCnw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "jOKjJIOYUi", "name": "glwbvtiEMl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 16, "email": "ayGEEPJFVc", "name": "DpUbuYszTS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 19, "email": "MBWWhoLGpd", "name": "AiPQRIllVg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 65, "email": "SkANuEfbKf", "name": "TKpATNWYvJ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 37, "email": "qczSkTdUOq", "name": "zOCUdEpokh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 92, "email": "KcZJOidLGe", "name": "THBfaZYBZo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "egwwHflouL", "name": "ykCLvlCYoi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "oEUsPxFPUl", "name": "wZPwdiGcpd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 68, "email": "wPNKLDOdhB", "name": "PxMujtBRxY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 96, "email": "uuiHYSUbeh", "name": "PSJtfvsxMn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "SWVxVYTjGb", "name": "BSstLUBGid" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "xeuCKZRNgn", "name": "GDioBzaVww" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "ayFUNSLydV", "name": "vEbmkMabBw" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 35, "email": "DJEJcCdBbJ", "name": "ahDFBTteNl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 94, "email": "tIuslFEYxF", "name": "MoPJoynAov" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 79, "email": "KuzQgnEsuP", "name": "CTPHiSTuji" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 93, "email": "mFkRSTNLKV", "name": "QaNCwrYRya" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "YnbHNJoMJy", "name": "eWQgQgpAKB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 97, "email": "jaPlMfjhpO", "name": "hBvAlVbkQb" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 95, "email": "EChPFEaiCe", "name": "oWcyMxuVwO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 85, "email": "LGVvYdIdxA", "name": "YdAAZxgRRi" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "fxuvxDSZex", "name": "yrhFuonnGp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "kxvrdQewsh", "name": "zhxbCNKGBB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "VvVCCYhrQC", "name": "bhEIDWhHCa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "uUbIruwggM", "name": "WgFvOTJIfG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 61, "email": "DOojAIehCJ", "name": "TLqMeTJacf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 13, "email": "ehaDofSnEu", "name": "ROezppcJUd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 24, "email": "EmPYQECJEN", "name": "iEArlvlSxq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 18, "email": "WToFXIgvoJ", "name": "HNoDcvJQOU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "XrvzjRHTQh", "name": "nDLWFyKcya" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "gRGYxoLPOO", "name": "ZAkGKGisin" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 25, "email": "HWzcynaMsX", "name": "XrniwhzStU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 38, "email": "eSCtnGKQvr", "name": "DYVOJXrEle" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "lyIoEzZVzR", "name": "BzDHQoTZlu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 54, "email": "egjDcoaFYf", "name": "awihVsndVW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 41, "email": "HTFwpfyhQr", "name": "DvvPfLAOxr" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "PpmZiNDDPe", "name": "sEItkBxLLY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "igNmIZgHWL", "name": "lAPOSKgGga" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 89, "email": "owODXKiRTC", "name": "DfXKZqvohY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 9, "email": "aFWeiobpHm", "name": "MhsVRWMoQd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 53, "email": "BqcXjUIzlq", "name": "SScsOrIpQS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "dpDyIgLSKP", "name": "kZbpdZlOOZ" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 73, "email": "daZEmVkesQ", "name": "sjnPACyodV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "BjtDDPjWBF", "name": "qjaGXgZOlz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 22, "email": "JpFREGXCVR", "name": "pBXDSdIbeI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "RJuOaTWomT", "name": "rAsgHPsEZF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "OHwXumIuEp", "name": "rGWNfOEAPf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 30, "email": "vUxJmXSufj", "name": "vyVcyCFHsA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 2, "email": "rIlDpPaOHQ", "name": "GTiObGyDrd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 58, "email": "nLNFttbGpi", "name": "THjQTtQwfd" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 66, "email": "feMPcZkHud", "name": "VgLEfzpyXN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "mNuubgcZtK", "name": "DIAyJXXsov" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 15, "email": "xJzERituJW", "name": "qsVuYERota" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 64, "email": "CYuUGCrmKx", "name": "anRlyCqVqY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 55, "email": "TzFIMQQCDB", "name": "AofKpaPwbe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "GgQKHjOyME", "name": "xkCHDNsUaA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 83, "email": "TJkKfbOJlL", "name": "RcmcLZxGIY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 3, "email": "zwCzmDlNeE", "name": "fGaBCTZxEh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "OLfGjpovbx", "name": "FYzEUtRgPM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 88, "email": "vGvOgDYKfQ", "name": "dmGuQJMpVN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "wzVyPKMsFd", "name": "ljUkYXYCah" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "TEaLBFwflq", "name": "CHRotlmjno" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "WBrfsQAZQk", "name": "ZoZGMUcRBK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "QIOFcFKoWA", "name": "FoGXVDluVu" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 94, "email": "RMJbzQvTyH", "name": "YxFjMnWKaN" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 59, "email": "KIjUWMLzbA", "name": "ALIVXRCnoK" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 43, "email": "SlcToMnXyY", "name": "rElyhiwxMm" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 57, "email": "kECsxkJfJG", "name": "yYebKQWukG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "ekAAIyvLyo", "name": "PgWEqbKXbD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "WjuEqABaeE", "name": "UXbuWQoaDc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "dwNiYvxDpg", "name": "gWGApkpxRl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "OpsduujlWi", "name": "SOagVIwcmP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 38, "email": "qMZfRzzqwC", "name": "eTgcQaArXY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 47, "email": "bqceMPjLVh", "name": "iNpniZdMsA" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "QtDFaTkYfs", "name": "wNzFnsOrxF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "UyLTCkHpKQ", "name": "xcHvqXSXAH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 95, "email": "nTDDwCsasA", "name": "BqHbAlExXo" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 9, "email": "cpARbEcJfd", "name": "gOiQktoNVD" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 92, "email": "xoCPYsajxK", "name": "JeBQOoHTjP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 83, "email": "ECeTzujbtH", "name": "djqYwjWkFy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "EBgbAjFrmR", "name": "YwqAjLNsPe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 31, "email": "DrTJEvGHPH", "name": "nBwKiqGAzg" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 51, "email": "SdChHomaeu", "name": "nLcWmngYOW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 4, "email": "lfcBPADGQu", "name": "OiVAGGqCmt" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 81, "email": "gCBnKuVsWt", "name": "XwEvoCweyl" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 26, "email": "dZfhhjLcQE", "name": "TyucPiipSy" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 69, "email": "gspkYCoSbO", "name": "LotJJwkwAp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 10, "email": "WBMxpRmYKr", "name": "WTWZezDFCC" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 33, "email": "qPuYBVMHOF", "name": "dwxHFnpmwe" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "LssQjwYdDK", "name": "wcFZbpggSh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 96, "email": "FbEZZlPBjK", "name": "AcdExeCEpz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 27, "email": "qVUGbTRtYY", "name": "kKPLsHLDkG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "ViJakkgdZx", "name": "lIlCkgShsp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 63, "email": "QChtxcdBNp", "name": "gicqgvOfLq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 46, "email": "plQvpwkJSU", "name": "xwuCmfVkfY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 30, "email": "xXRUUmKYYA", "name": "yjjHlLSlYI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 90, "email": "OabLbhmnos", "name": "SMiSCDBjny" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 12, "email": "GcMnIIWHLc", "name": "TCwxBcajyh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 3, "email": "ZDXIJLriLe", "name": "wSqudjoUFB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 36, "email": "vBPFXHDYJm", "name": "UOJTZwbkjF" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "QFJtyFjJfI", "name": "WCRHcdruoS" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 72, "email": "SVlaDNLjua", "name": "MzQjIgmkjI" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "NYeFsivkox", "name": "nSqAGEzHXz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 87, "email": "ijFvkMxUha", "name": "CHrXlvBjSW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 36, "email": "cqZQkiBMvD", "name": "RIIIlglNHv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 23, "email": "yoICekbEdq", "name": "TJopOgsvpU" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 34, "email": "TWjYznKEUF", "name": "oZPJXLJITp" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 88, "email": "SbqpqFHWeY", "name": "AnKswYGaOf" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 3, "email": "esTQGQGhDJ", "name": "riisiyxlVn" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 26, "email": "wbdUizbePr", "name": "LnUERVTnyE" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 34, "email": "coqIQRLoBD", "name": "SnhpheAqZB" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 83, "email": "CnFXxMYCBP", "name": "yqDkMhcstM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "ZEhIBoutyu", "name": "sQRcpYQDZM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 38, "email": "UuVOWYfFdG", "name": "cYILyKPhPW" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 71, "email": "zcVLorCqoy", "name": "gdGrRwHJVq" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 32, "email": "EiEhjawYCo", "name": "HCaXsbByOv" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 79, "email": "xiUOZLJmdw", "name": "zGvYraeLEP" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 21, "email": "YpBoXQRTbB", "name": "RRQxcsHREh" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 1, "email": "tGtZdoFobt", "name": "aJRFxmMftz" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 8, "email": "yxjMROvpvI", "name": "sgmqzRBKDG" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 39, "email": "pjrihBiwoJ", "name": "qAdBidtiIa" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "NtFbNTGEPU", "name": "fmszCGrdge" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 11, "email": "DvAyNaPWVA", "name": "NgbBGDEfUT" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 6, "email": "SlGebdgiYa", "name": "mruvFphRLY" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 60, "email": "VibLKmWECY", "name": "kzrWgUCakc" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 98, "email": "bpKPOdknTh", "name": "AlWithecCV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 49, "email": "HIfYSFsHWj", "name": "bXjqaToQyV" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_simple_actions_4() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 5, "email": "xmgxGAnuKT", "name": "xaldfSOMeM" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 52, "email": "WqCrNBgMwE", "name": "qXZVWlneUO" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "age": 99, "email": "gBVuKXOUCZ", "name": "rQyEJXVgFH" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Benchmark {
            base_query: r#"
                    query {
                        User(filter: {age: {_eq: 33}}) {
                            name
                            age
                            email
                        }
                    }
                "#
            .into(),
            optimized_query: r#"
                    query {
                        IndexedUser(filter: {age: {_eq: 33}}) {
                            name
                            age
                            email
                        }
                    }
                "#
            .into(),
            expected_data: json!(m::object(
                json!({ "passed": true, "repetitions": 10, "factor": 2 })
            )),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("go.index.query.performance")]
fn query_performance_simple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: [
            query_performance_simple_actions_0(),
            query_performance_simple_actions_1(),
            query_performance_simple_actions_2(),
            query_performance_simple_actions_3(),
            query_performance_simple_actions_4(),
        ]
        .concat(),
        ..Default::default()
    }
    .execute();
}

fn query_performance_with_float32_actions_0() -> Vec<ActionInstance<Actions>> {
    vec![
        NodeOpenPersistent {
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                    type User {
                        name:   String
                        points:    Float32 
                        email:  String
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: r#"
                    type IndexedUser {
                        name:   String
                        points:    Float32 @index
                        email:  String
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "cUbYhiZzKa", "name": "KBleeANsoc", "points": 35.284447 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mIGncKyRWX", "name": "evWsHOIPgZ", "points": 11.372711 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GAxhyuoHkC", "name": "AeZencueQp", "points": 24.304092 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MgphOrTZQu", "name": "psLnyFJUoS", "points": 82.26058 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KfMkiUieee", "name": "mAbOwbdHLT", "points": 91.043755 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FRsUYSMOpl", "name": "xluSTBqYip", "points": 4.6626782 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tXOXQZdikO", "name": "NfkoAfLlTn", "points": 2.0438662 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fIdFaoQZUY", "name": "jolyGkIcFO", "points": 40.315075 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rOkbeIaEgz", "name": "bRulHnKDsq", "points": 14.783686 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jJaQWSlCNw", "name": "kKYYRqIKkX", "points": 47.774017 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GDkjzcOurI", "name": "iRDAXAWwYE", "points": 80.10757 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ctdJoWykRg", "name": "lNAXfNsNzp", "points": 3.9844043 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dYyAzciyup", "name": "CUZrZcSBeu", "points": 62.600056 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VpgnfUZcZe", "name": "DtCBxAsQax", "points": 25.697266 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ClpmGWSgPb", "name": "mhXyYNfVBf", "points": 62.506664 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "efFATPGhRp", "name": "oaMnTbgdDg", "points": 87.05867 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SKQKyZdLxp", "name": "ucbMQSsNml", "points": 50.695633 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mLRLkzrvSB", "name": "fsXkksVLNs", "points": 50.144127 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gXOGtRuhmN", "name": "xDxytPuzGI", "points": 25.999565 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xgZtyXOEdt", "name": "yyMZxFzfIe", "points": 18.432993 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qAchgANUOu", "name": "PylbaSjwEU", "points": 76.21286 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nBsDxbmuuQ", "name": "BwrpZigekw", "points": 98.806076 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HSQAWUvTUY", "name": "GNDPonSHab", "points": 76.57502 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TsqACltMsY", "name": "VZFWHZWaPB", "points": 95.768585 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FoNFClyXbb", "name": "tAKuKSrWoD", "points": 39.8112 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SxOrJiIoLA", "name": "JIMahKEUxW", "points": 50.457172 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SHlURQShsn", "name": "TEviBOQKwg", "points": 94.01268 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qaNlzYgrwI", "name": "fbdyPSpEfv", "points": 75.80389 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ahPzvEgKWb", "name": "ERnAaIgzsL", "points": 12.310212 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vJXSZbqMJt", "name": "FgHmfkHsCa", "points": 2.209621 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TZTXlqntzX", "name": "DLFATibLaA", "points": 10.80403 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DeODHaEmpR", "name": "xkKcOwkrru", "points": 4.7403383 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pQqKTWFAtM", "name": "jMkghbqXfr", "points": 6.2266073 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "cnlbQEtZaw", "name": "tnHEGbURyc", "points": 24.346277 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ivTYLQBFub", "name": "yoMQRJRWtP", "points": 17.273737 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vQjbCdFHjG", "name": "QcewayLxXh", "points": 23.070995 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WYrqtNnmCP", "name": "Ialhpbdqhu", "points": 17.151833 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QSRQfshYdT", "name": "pMUHWwmnHp", "points": 85.78339 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uQLLUKXLxN", "name": "kGEqsXrSYC", "points": 49.972908 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EsnyFkLPcG", "name": "xesIxlqphK", "points": 52.71954 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uJbJXKzZIY", "name": "hYKNKirTPz", "points": 73.25725 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ajRCfFdCby", "name": "DLOCvLNUii", "points": 48.2015 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CXcpZGRQmo", "name": "JkfpxDkPwC", "points": 42.497044 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jNcUdYFWnH", "name": "BjgoAFCsKh", "points": 35.88879 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KEItQiYdDt", "name": "LZfewzqyJO", "points": 67.533775 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fAOmzIEPNq", "name": "MbCYzPDLWD", "points": 47.59082 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UAXdiCOdIO", "name": "qZuWCNkMfi", "points": 45.44084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WEmQIzqsSh", "name": "VMghmLxEDP", "points": 11.076138 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "XjkYrHJDIe", "name": "fbeDcgtdUZ", "points": 93.176025 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BfaQIHEiTn", "name": "TDOByaIlDk", "points": 90.252106 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LYRSsoePgR", "name": "jDTsdcwlVW", "points": 17.10032 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NSkGSfSmdB", "name": "NvugkEsTfY", "points": 69.81551 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VSPmAyeNzG", "name": "ECZjWNIsqq", "points": 42.047848 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JRgxYHBbEz", "name": "utvGEeFZgT", "points": 6.7015424 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PAAKNTqEng", "name": "RnyPaGksXb", "points": 33.063454 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qtUfSNLFgS", "name": "eUiFQnixYv", "points": 55.84057 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "yABuLqchsI", "name": "EkWJQLINqh", "points": 8.171223 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AeGzXSweJN", "name": "YbgofZkMWk", "points": 36.70726 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "cVOnruoiUZ", "name": "WQkLJKHLwZ", "points": 10.666018 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qxiWYVnalc", "name": "yznGDbMmOo", "points": 74.81791 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LiwRPLFMpT", "name": "hPwDZdqAve", "points": 49.024994 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nwdxyJEput", "name": "SimFsdbuyE", "points": 57.15481 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MJQXbjEhmA", "name": "dhXfVRInRs", "points": 66.94435 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EXekwUzshf", "name": "WtLcUBsbgU", "points": 92.8648 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WUMlapDSwD", "name": "uuvgyDRDUw", "points": 42.338932 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PKLZOnGCiH", "name": "smRNadsNoI", "points": 67.03008 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dImPfiExwl", "name": "oDsFQUnQIo", "points": 28.931952 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BOvWngOsZM", "name": "YDQMTaLVtG", "points": 63.555653 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DtcSEDOmDz", "name": "ZfhsIeRiwr", "points": 97.91006 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AkbGlRLgcj", "name": "RYiRJgxPUi", "points": 77.08571 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UiLeCWxfOy", "name": "NDCaSTtUcN", "points": 33.3744 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HkqwjMBxgj", "name": "EfkpvAdQhK", "points": 62.60952 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UTOVDtBkNm", "name": "EqqmIVxufZ", "points": 5.507842 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VdnBNqLIDS", "name": "nwTcRBCpzj", "points": 14.881215 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DIfIdzsGJu", "name": "vJnWWrAAra", "points": 12.420686 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JMBAsmPxDb", "name": "xezTfgNvEQ", "points": 93.55725 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tpNJoPrbEu", "name": "EczNUHWQbR", "points": 88.26602 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gvdvmSmzBh", "name": "zXZnuGMfNX", "points": 20.662933 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PdtjsXzsEk", "name": "JNROkRVzpt", "points": 71.03889 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UcgGjXmoFO", "name": "KVXIPfiMdb", "points": 39.231476 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LvQEHstTNK", "name": "tlbSlwsTGe", "points": 62.388775 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tFDrITQSic", "name": "Bpegqxiajb", "points": 12.953976 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iHBQMvRhwC", "name": "PFMypffIZY", "points": 75.76373 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vvbzTBmlGL", "name": "lUeidmSWOd", "points": 20.318552 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "eqijVsHSNG", "name": "MLaDDADOui", "points": 63.355858 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "woANGdwTTY", "name": "cCIzqJOdDc", "points": 94.85239 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "deMujXWvCv", "name": "WKAEcnbAnO", "points": 36.30336 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UxxErkOFFN", "name": "FvzHDEWziS", "points": 97.761536 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uJzXFnaADB", "name": "mLFwutemwe", "points": 65.29551 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PgCWwdiwoM", "name": "CiyQkiTwMG", "points": 40.9678 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MTdeQKiSuR", "name": "nesqvDMJuK", "points": 93.00088 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ibXszfoujs", "name": "HToiSepRoY", "points": 12.659545 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZmCckSuWdJ", "name": "YzuLiJoGQA", "points": 94.61623 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xGQUlNQYJG", "name": "qszYFZOqUT", "points": 26.66654 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ehTgQXBbco", "name": "UFGMIMYlre", "points": 65.155396 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uZHZSuPsnI", "name": "JkDsCWOytb", "points": 90.111885 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "sURGjTlAgR", "name": "EjXLqKmevG", "points": 77.46235 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gjpplgMrNo", "name": "weIIXkCgsk", "points": 82.6709 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "aULFULycnX", "name": "lurGqykONX", "points": 49.66702 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PKoTicLTRB", "name": "amtZvidPbv", "points": 73.247025 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kvGruHVrjT", "name": "PKwJjvFkON", "points": 31.682064 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ihFrJxpoKt", "name": "DznrSpjmSZ", "points": 33.269085 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EkqnhnMRCi", "name": "XNqToZVGbM", "points": 37.695957 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SBhDLSRPrA", "name": "dbpkxddmCW", "points": 83.60685 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JBpEQDRviZ", "name": "goWoicwkST", "points": 21.5507 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "oxRRewBrko", "name": "aPQsGASZpG", "points": 19.930956 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "cccpZmUgLE", "name": "qFUgrHeool", "points": 62.951088 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZqmeSqfOME", "name": "otsDglQFxA", "points": 98.60258 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AFtyfJXIAD", "name": "HusyKktcFM", "points": 58.658188 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iyzBDNANAd", "name": "FbLhIDuiVB", "points": 76.99835 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JczKcBwuQf", "name": "cMBHgkMhht", "points": 16.396935 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CLaCnSxhce", "name": "YuGowJkMys", "points": 81.67878 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tkJuvoUFFv", "name": "GXZmoHtvjb", "points": 37.31513 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VhdmEzkzyg", "name": "sPCjpjCVxL", "points": 98.329865 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IVmWfTLnoW", "name": "lZgbitooVF", "points": 88.20753 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qgblGIQmav", "name": "yrUJqciWKH", "points": 32.05422 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pfbNzaxCzV", "name": "eluOufmfKq", "points": 82.39119 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jyxtRFXWvm", "name": "LKrjOoSogJ", "points": 13.013207 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RuoxOSPdNw", "name": "bFDcAWgGkV", "points": 85.0174 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IgTBiHQzYJ", "name": "JySJGrzAUt", "points": 9.601152 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WkoLgKRhtx", "name": "DeNjYFffAA", "points": 77.050285 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dHfrcGDUyF", "name": "EfchBRkCoM", "points": 36.98325 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BULoVoktsv", "name": "GBqJxwoEjG", "points": 47.123455 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DjpHDDzILL", "name": "JzMVHwFNgN", "points": 43.743553 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LmTXdNnrMo", "name": "dNjMMJDpAL", "points": 91.40906 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lsToirbYjF", "name": "EIqyAnPDmi", "points": 3.819822 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MwpsqRzOVN", "name": "CZztRpUgqM", "points": 77.618385 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WAIIgxZMAS", "name": "fFxbthclgv", "points": 41.43095 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "hZkoUeHNCH", "name": "urqutukUfu", "points": 63.27157 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "yZwXXUHXRh", "name": "AwgkCRDSLz", "points": 0.3958806 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PrItJTIqAL", "name": "BEjzAcLQxo", "points": 24.09188 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KjXLcKTOLk", "name": "TbScwfhVRN", "points": 68.253815 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QmguZwKaDn", "name": "AIqNmDDwOd", "points": 76.05488 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OPfQAfZgAn", "name": "zUylWPAZSO", "points": 25.86868 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xbkpDzNiau", "name": "vyJjttALsr", "points": 20.669083 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DMndsWxXJD", "name": "dIkqBmfPZF", "points": 40.14077 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "snJAlsmmZH", "name": "EcvgZDcohQ", "points": 77.26229 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GLAGbXNVWk", "name": "XOvXgLKczv", "points": 41.09373 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LTYEIedbqi", "name": "SPBExYmfVt", "points": 40.635662 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mIrUpNqUXA", "name": "eGhgfYDzRo", "points": 63.830196 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WdxdghDQan", "name": "MUvmfwdfpq", "points": 20.823433 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tuFeqlOjjF", "name": "tfhDNPVWRR", "points": 72.81214 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dNCZhufejX", "name": "dGvVHMDEDK", "points": 27.844833 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DQqmvIWOom", "name": "IFUuOriwvh", "points": 30.566185 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MvEWTWUjLB", "name": "OLiHhDSeVF", "points": 65.123055 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tCGuYBjxIy", "name": "xxmuzJopUP", "points": 61.12729 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lLywyEXUNz", "name": "tenbOGHJUi", "points": 14.208742 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LzYiUDdFcC", "name": "YcMIzZmHRa", "points": 20.588913 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YnvKKwXjUc", "name": "xssHDNZjte", "points": 70.627205 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iZQMcbGlfL", "name": "UNtnHICOWw", "points": 14.405845 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "McCOESOBAR", "name": "ayyqSQBrfX", "points": 81.911644 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YiUSuawxXK", "name": "LzAaMCwnPz", "points": 0.064469025 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "eSxaPwDhjB", "name": "tBPeLMJPrB", "points": 66.77171 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QDdQtzqGFa", "name": "xGhWkMEssC", "points": 45.22054 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mJqYrjXqqP", "name": "mEkRfUhGoE", "points": 56.32396 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "sgvQHKCtMy", "name": "CBOMdSTrSG", "points": 20.412218 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NzwmWvZGuu", "name": "IjPeReptdR", "points": 38.47167 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WlaCcKxRgT", "name": "ffdbTxXaYF", "points": 31.353449 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(json!({ "email": "rzfphzahjU", "name": "DkqlEeCOhg", "points": 44.869 })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TeiRIxyipd", "name": "ffNecxfzfS", "points": 55.830147 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kMbSOUOrWD", "name": "QfKpjwdWus", "points": 86.10228 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fHWhuYBwWn", "name": "pHENhLqPQS", "points": 56.106155 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PsTuEJIbgG", "name": "sBqFhIXfjT", "points": 37.349697 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WsHQUCPyxR", "name": "KItJUdvkpA", "points": 88.34409 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lNOrSzHDCV", "name": "VPEnRTYvat", "points": 96.56616 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OUIsElfPNf", "name": "OkOHpatiYi", "points": 83.49599 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qjGOJXnQDP", "name": "ufQkxrFOTP", "points": 81.80209 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NPZEXlnbjF", "name": "NjHrsCFapt", "points": 82.05547 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mDLgzEAUsf", "name": "KkULHUmhuD", "points": 20.291391 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NefuAlBvIQ", "name": "dXVcUkgKyk", "points": 61.270687 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "eSqNRvHYAY", "name": "mJtQOtrSGJ", "points": 36.9591 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UyPlPgLHOc", "name": "AfXYhIHyqm", "points": 67.58832 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "krfAhNovYl", "name": "ZceXkTcjZg", "points": 52.59761 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rkwrkdcTKR", "name": "AMtTbBBeFM", "points": 21.608015 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nExlfwLwVn", "name": "dbGYXalgoG", "points": 69.6982 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QOTBCEYAAK", "name": "FDHmOTRQsg", "points": 18.364685 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VceWXJtawU", "name": "YSTJhmmlKb", "points": 7.275813 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jxzNrJdhZn", "name": "QZiCtETcXd", "points": 96.31852 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gHnuMZkAHT", "name": "KxYSRqpEnU", "points": 81.2267 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vANixapIDn", "name": "TwICYkLLRq", "points": 78.36273 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jIWEVkMoUG", "name": "oLtTqsuzzx", "points": 7.3437614 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lDcfdMKXni", "name": "bphZTmfkjW", "points": 32.493023 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RVyTsShYgZ", "name": "NUHraDyONY", "points": 33.535034 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vQkzqoAOMf", "name": "xcymyahZQE", "points": 95.20221 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pRYVsAZfrE", "name": "pubtDBcHBZ", "points": 59.290977 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jgExguIrGW", "name": "PhVccmgarF", "points": 19.664799 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rNzDkViosy", "name": "EpnqBBzmPd", "points": 85.85582 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tvFKIDxTbx", "name": "unkcbrtnec", "points": 91.76129 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AiBACNoEFs", "name": "UjJJwVykoT", "points": 83.36462 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "zwTQnbpEOC", "name": "JiEgMLjfsh", "points": 70.66919 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IhvLTHauQx", "name": "voRoKtmxmT", "points": 96.596756 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BDRGWarICu", "name": "adgBqYzagR", "points": 79.09543 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FrHLPyNAPx", "name": "nfDGuvDOBC", "points": 98.85384 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YaOGCppjrY", "name": "TZNoRNXvXN", "points": 10.354457 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UWGqLdaccU", "name": "syxCmWetiP", "points": 28.48741 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xlisGkKpdf", "name": "fGfIGUUgWM", "points": 46.37259 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WvhmgpJWbQ", "name": "GtsPeCxsIs", "points": 12.164135 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CYxcTWUMEr", "name": "MglPQlwlsN", "points": 74.390785 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "oAelIsTKmU", "name": "mLrPiyhkGK", "points": 44.351917 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IcSBtwIPhK", "name": "zgEmTVIxNA", "points": 53.366585 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iHWRVNlWBf", "name": "UOUnlsJzmO", "points": 82.17927 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GkIoRKOsgu", "name": "BhweyfarEk", "points": 18.135147 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WOeUpwCXpR", "name": "dEGjQQOSBu", "points": 7.8432856 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fsdJFdxIUy", "name": "bPORVcAFJJ", "points": 55.07545 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "veWkUdtVKz", "name": "jRwXKOuIzo", "points": 11.4631 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mPhvjyMXOS", "name": "IrmTjioQzn", "points": 59.319077 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZpfpFxQYhA", "name": "DeJTKsVzzw", "points": 78.25107 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "hTVRVDHWMs", "name": "qrQSCBTnnM", "points": 98.983086 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lWityvQfWB", "name": "OjGuUGnwoW", "points": 35.46879 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SBdZTDfDSC", "name": "aOkhbEXrtD", "points": 35.169643 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FddqfTiViG", "name": "THeEWwFJog", "points": 62.78722 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pWxFrraJaq", "name": "wdUVjPxjFH", "points": 82.35741 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TdjQTkXwgM", "name": "AtFOaUyDur", "points": 10.852384 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IfoTVSfcsz", "name": "xhaPnaMbwz", "points": 90.29033 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MIrlbcXoxd", "name": "wMyShfsXIC", "points": 80.58325 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "XroramUwwh", "name": "SwKKpEDyNp", "points": 27.700338 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mFnPcXzfRo", "name": "OusKQpgjOk", "points": 65.46928 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "zEyXKkAnFJ", "name": "uATSiBIBgq", "points": 21.197706 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fGvYaFAAZb", "name": "nZBhsoGzGV", "points": 11.108653 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "XDBzhSHveO", "name": "uUtPXPhssY", "points": 10.963975 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BzRSPmkqoX", "name": "xvhtEdaHnU", "points": 85.73382 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "THztlyGCLj", "name": "UQCPaXPBVw", "points": 29.502623 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iihoKpZCnd", "name": "pdcxYZwfZa", "points": 20.552538 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OVOtfWMPAH", "name": "rjTACPpzuE", "points": 41.57589 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZoFQcHFrHX", "name": "BncPdVuyWf", "points": 33.309727 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CqpfCJBmNI", "name": "uKJryPIyET", "points": 80.34064 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ghJQrFJteh", "name": "lzQamOGqiq", "points": 51.3265 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RUBDVMjiJP", "name": "qdHrQSZjhk", "points": 32.518826 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QvHNlbbgyn", "name": "ZJfGAbTYxa", "points": 87.35228 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iwjCPvCNnc", "name": "IHEEbqsYLs", "points": 72.26261 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KTRkzCjHgs", "name": "QETXLSnMYm", "points": 74.61661 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LAnuDesrEh", "name": "wiTXIIKwtD", "points": 62.369514 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "amIfhAewgE", "name": "LLRRZutEDt", "points": 78.8554 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xmMVKJUQyO", "name": "wsRJYobxqh", "points": 86.503044 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CEJuAvWvWA", "name": "IlLYQpROCw", "points": 74.60727 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QapMBnhcQi", "name": "YyoPvyyBOJ", "points": 96.40393 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lNlzpvzKRY", "name": "hEwDYFfmCP", "points": 66.3713 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ijFOuMevwJ", "name": "KHXYTskWVX", "points": 41.30298 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GpLlkmVOpZ", "name": "FSKVXgcDUb", "points": 53.486443 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LZrmzdRBAP", "name": "qdowuyVxTG", "points": 10.778252 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QzTUxYLaIu", "name": "kYQgnEhsxF", "points": 35.91334 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BlcAiaKvYi", "name": "EyIdBhPpBI", "points": 5.645603 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZSGvUxxmLM", "name": "JxuFOrOZPf", "points": 25.772306 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "bGvmwQHneG", "name": "pxDFdryQfM", "points": 67.11705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BRnjxAryZE", "name": "ITjdmgxoBP", "points": 62.925396 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "oEmEGiVLUY", "name": "FYHgfEJqVS", "points": 54.408047 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xMwIlXbbvR", "name": "RNCkGQhOPm", "points": 5.1631813 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_with_float32_actions_1() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ADJZCyxYLz", "name": "LsGOZPymqt", "points": 89.37627 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rzYgLJOtNc", "name": "UolFetRzgk", "points": 91.58994 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "chbnzopjQW", "name": "pYaHAmtgEk", "points": 54.879265 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IDRmbRPESb", "name": "mZzVJvHtxf", "points": 49.425297 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pgwAjqiuhF", "name": "jplxulbEhr", "points": 95.08161 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "btwKmvUiRB", "name": "jEsOKXRDQi", "points": 5.3460016 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YSyBdptYLH", "name": "nqwDHcdyii", "points": 83.815285 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tTFjlmxGVu", "name": "PrISkFtLgq", "points": 70.83841 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JkYDgteuWN", "name": "baGVXkzqty", "points": 1.3391222 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fjTAiASdWK", "name": "yquxFcFbpf", "points": 72.59394 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KhaNbjrnAn", "name": "NclfHGVoTA", "points": 53.69187 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "agcZLVipfi", "name": "kDlxaevlWC", "points": 49.96235 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RWxkuhJumw", "name": "KnnETeICFA", "points": 54.446365 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NAneVPEgJI", "name": "FSpxJNtPYu", "points": 55.477985 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BggiPgUzyq", "name": "nfNCxzWvfk", "points": 43.659798 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rFoXWGqYmQ", "name": "hHAxCmlnwp", "points": 22.064602 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TATkzmofdn", "name": "AApMcjjPQy", "points": 52.664543 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ESzOSVZCfe", "name": "yfGySNrPPC", "points": 45.555912 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nSMgnfoJBf", "name": "iiqtrLuVmq", "points": 71.37458 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EiTQhKsjvR", "name": "MoqxSZYXuq", "points": 65.52048 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HGrYhDoFAm", "name": "AnFZmjPopE", "points": 80.66662 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CwyGAVFLOR", "name": "RZbVSHUyYR", "points": 94.494484 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BGqzwBBxJl", "name": "QYIEMvdGCG", "points": 22.223005 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NIjNLEqXen", "name": "wfNnwyZyqA", "points": 96.66595 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GezZiKvEqG", "name": "EKarreMqKg", "points": 61.53291 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OKwDDuqDBv", "name": "QSTWTVNpoE", "points": 23.149176 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CKngOPDKsd", "name": "qVdDhbwhKl", "points": 71.0912 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qKNrKuOaGo", "name": "wmTgJQRnJj", "points": 92.92071 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pnbUAILQFE", "name": "lShPmaCcUP", "points": 21.893673 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fzwIzNCgPi", "name": "hpQdcYbLMr", "points": 14.609453 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fAkaHophCs", "name": "mnLRlOxCyh", "points": 66.88234 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QIOufpfFRe", "name": "LrRintBgRA", "points": 64.5909 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YWwHzRNkIt", "name": "lLBSzUYdsG", "points": 92.137115 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dJApqXwMme", "name": "LmZkPpWGuG", "points": 46.216713 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZJmbuGZnAX", "name": "kNnumlnGRQ", "points": 75.02772 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZKVrPbmHyB", "name": "nCRirvNPHC", "points": 70.72888 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UtWaKklsUF", "name": "eYXusjHrwL", "points": 39.427326 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FjFUmesrri", "name": "NFzBsWIrtl", "points": 11.343788 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HNEdwZnnBo", "name": "kSzZzyEuQp", "points": 45.063232 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DxQpgceAfH", "name": "CDnkFRpHWl", "points": 46.614468 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tlyPuKviRS", "name": "IlvMebqskb", "points": 62.039898 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NyYDkEXOqR", "name": "zZCTUfhfSf", "points": 5.6969852 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UfUZewlquC", "name": "YMARvqagUT", "points": 29.197165 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mJChBpqZrx", "name": "ZPLTijDMuy", "points": 81.03626 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fFmCvQUYAn", "name": "lqVWyjXwcW", "points": 70.41452 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RhywpyQaLT", "name": "dyDElKOdNM", "points": 80.597755 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "eytkSAeQKM", "name": "TKSAgeMLnT", "points": 42.797684 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nPbJJpBtJH", "name": "cJhfGhzyTW", "points": 94.98361 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uAFDKkIbls", "name": "faKJStiHEj", "points": 92.66647 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lsGDZsohXu", "name": "KzMDhyXWCo", "points": 31.576805 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "FsgsusIKLh", "name": "qAsKnWGjdM", "points": 26.898987 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AOahXxWasn", "name": "uwNWeHIDJE", "points": 9.361189 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "oNewYCLOeN", "name": "psKIgqhJau", "points": 18.49427 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "SNjzQQMqYC", "name": "kDNQsUaaPP", "points": 17.089685 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jDCYciaMhm", "name": "tVwrjuYJMj", "points": 97.111855 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JOIPkgfWKa", "name": "EqkSSxmTjk", "points": 93.55695 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BCOBSSXoOm", "name": "uxulBZOWEq", "points": 5.606011 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "hcMlhiACMH", "name": "tghXbmROrD", "points": 69.11994 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "osXkRFhiui", "name": "UBhDOvcvjS", "points": 86.84705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RaCbJjGLWZ", "name": "FeeogGMSPm", "points": 66.34861 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "bLgAOxsZWB", "name": "UtuzhPrBEx", "points": 8.695737 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QiGvxXTuZd", "name": "GTLEQYcnHO", "points": 93.71204 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MIWdvHAOkH", "name": "mKdTGfsCbo", "points": 48.37009 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ttAYtFjHVw", "name": "EwEWADxBFo", "points": 41.543976 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "wfsLMHvChP", "name": "mFfrRwqISz", "points": 19.770504 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NNPHLVhtin", "name": "PNEuOIEvoZ", "points": 39.128395 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "yIwgKhFzLJ", "name": "KGIpbluNgZ", "points": 29.53613 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OjBYqYCaTe", "name": "uwyPOrrBlN", "points": 94.802986 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AequQHlEOE", "name": "ScrmJGKHhf", "points": 55.587517 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "arjNnbqJZa", "name": "QkamSrZzmd", "points": 82.319336 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HGzsGytVOG", "name": "WCDFLUlTJy", "points": 65.30527 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KTEGFWCnUU", "name": "WsevqeyQoe", "points": 8.960042 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "thGTajzLyq", "name": "SXmyKVhTXT", "points": 30.645876 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ooCSkYvmiR", "name": "rqTJHOVxOp", "points": 94.34141 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "phWTQYEoEB", "name": "MyzQgcraeT", "points": 85.231804 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HvqfbyMAer", "name": "eMMAUsFXxB", "points": 23.13097 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vFjQUAupAI", "name": "MBNFBETaSI", "points": 89.270485 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DJcyvkjUnQ", "name": "WRYLBGxtkK", "points": 34.043873 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tTqBBEGpuo", "name": "dTMTMgIKdg", "points": 40.501507 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gCQztuqiEM", "name": "ErVmHXxAFz", "points": 30.33604 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qqzXOZwnod", "name": "itCwxwafUD", "points": 14.8796425 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EAoWdXqTGy", "name": "enaDtqKbjz", "points": 8.304981 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LtBfCUpNLf", "name": "gDsIuTqQEn", "points": 96.32396 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kgBAjlchKe", "name": "sTNalDXdni", "points": 6.8629627 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tlOpLSPnzN", "name": "INWSCzIbdF", "points": 21.478203 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ilyLXaoXTO", "name": "mjOxbtnjtC", "points": 91.43038 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DblamAKwdY", "name": "bPxxyCmbyf", "points": 76.40052 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PrqbYJZYyd", "name": "vDnmjxdubn", "points": 7.0828824 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kwapILlclY", "name": "ibTIHMXMYf", "points": 82.77103 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CHAQPAqQub", "name": "IDxvTfxxbb", "points": 76.97041 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kGcvQEKAwl", "name": "kvqJETfKmZ", "points": 88.17882 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RWoiDsRzHB", "name": "hoUAGkKAYA", "points": 38.379524 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HcLHecOFji", "name": "LPvrfaEqmy", "points": 81.025276 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JDOgMdEFAI", "name": "PnWuqeuFYT", "points": 5.0024886 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WKCtlSaIvO", "name": "JHONYMAavA", "points": 3.8243628 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "hbmPhVDZvL", "name": "OSuwrHYQKE", "points": 9.396372 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LGtaolVmeB", "name": "skeTCeLsQk", "points": 98.43129 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JFgyzYDSTP", "name": "aBibPhPIfi", "points": 2.201345 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TAoMoJPzmZ", "name": "OXiqauzPnk", "points": 89.03698 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EtuzGYydZA", "name": "hvKTmvybtQ", "points": 18.836472 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tAxzcWUTXY", "name": "AZESIosbIH", "points": 4.9891768 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QpCahwKtFW", "name": "XbgEqiUFkV", "points": 55.815937 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GAmOkoykoT", "name": "eOeLOfmNUS", "points": 87.62729 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IcuWXupbKl", "name": "JlxnVwLgoj", "points": 10.890312 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iEqiqTytnn", "name": "UOPSNJhDlx", "points": 10.134236 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "sTGiyTonxm", "name": "auzxZbYBWb", "points": 72.414894 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LbjrVARXYc", "name": "pAaCNahRWm", "points": 84.02953 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mNeYuiXXzH", "name": "XBdvGYEJBH", "points": 30.698767 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AAhjaehLCc", "name": "mHEtOJDqhU", "points": 95.95265 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NgKZKbLpnw", "name": "VjBpDMGWhP", "points": 97.00397 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AekwrSkTzE", "name": "aeCsziBivv", "points": 62.13024 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uFQifZBEVC", "name": "hGDkTlFdmu", "points": 64.87983 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IvgnkgihUW", "name": "UXWxkaLLcL", "points": 35.885296 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JQjUtkViGn", "name": "ZtJrSRwHRr", "points": 93.18328 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QacbYVgKSf", "name": "nahzDUnixa", "points": 12.211342 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uczRUnBKpl", "name": "dNeQDInuHj", "points": 65.04909 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YFsFztqHgr", "name": "zajLYtNtrV", "points": 88.47979 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KTAvCylrYQ", "name": "qxwhKvIlMB", "points": 47.01808 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ulvkOlkQIr", "name": "xrsDYzCCrO", "points": 40.80809 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "pZCyGaPjjx", "name": "GjEwvKttCe", "points": 78.211395 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BVHTljgziV", "name": "SDJFSaejbQ", "points": 36.1722 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NKllOlTwnu", "name": "jAluVasOig", "points": 31.176895 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BnxEpHmAFJ", "name": "ufSWXTgPNL", "points": 83.16767 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dxXRsAhHLm", "name": "rucGCLWIVC", "points": 81.516945 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "iuzeIYXplX", "name": "dEUtRbSjuI", "points": 21.914913 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EXubIVnknJ", "name": "ZsbeVtdWrX", "points": 92.15432 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "sRkTXEqAjP", "name": "ivybkzdrUM", "points": 53.313667 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GpTmpWUJiE", "name": "XTKTEJBsrF", "points": 3.5459797 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qeHQVqdlwi", "name": "OHrlbliNnD", "points": 15.587944 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YfWdVOnhGq", "name": "FutBSuvDen", "points": 70.69857 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OTkRPRRnxS", "name": "wbMpFbShch", "points": 11.701991 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HChbPwymNH", "name": "edJkMOYXSx", "points": 4.910091 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JVASxEAWuJ", "name": "GZcvgRkCEO", "points": 84.10024 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tGANmCuNFm", "name": "HYxpCgSGIj", "points": 15.17969 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uSJHEbTloO", "name": "JYMYvdpsXt", "points": 6.173009 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mgVHIENDVL", "name": "uIiWeLdDOA", "points": 25.12261 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JYrCRyjQJp", "name": "WPlgIvypAF", "points": 72.990746 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qEAKslbUyg", "name": "jAkdvTaGvY", "points": 49.373867 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ozfcuNdGKu", "name": "dkEReRMnKy", "points": 98.613594 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CPBLJxQfLw", "name": "mpAjLjaMSJ", "points": 15.79021 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YyYlRAPOpN", "name": "AdDeLpppvP", "points": 87.49696 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "wsRWdazJTH", "name": "mHBOpXkyHO", "points": 12.585742 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JZqoQXwWFm", "name": "pLlVoEnUFe", "points": 58.330856 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RxipVdlVLu", "name": "fcRjTMqhue", "points": 44.44907 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JuEoThqaQX", "name": "jTPTBLhhMX", "points": 16.425667 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "EdZOONGovl", "name": "PnCOUleZFb", "points": 52.36465 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TbFNxUsEVJ", "name": "GfKLMrghzg", "points": 52.63037 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rEaTPuBeog", "name": "EeLxYiGgVJ", "points": 52.257843 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rFeHapyDCx", "name": "AHDjaGGkoE", "points": 24.33134 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rRfLYzUoeT", "name": "GvGINgcfGs", "points": 95.30452 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BroWmkpojj", "name": "vwHGOjLMCb", "points": 15.764127 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MeWmioISHA", "name": "VgdnlGABod", "points": 36.05548 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "bGwNtHBrFs", "name": "pxdxISyRqS", "points": 77.804695 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "yejLqQbnVr", "name": "WxXHEymYht", "points": 3.991705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RtAuTkoUua", "name": "noZiUvdwGg", "points": 51.73511 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "USPuJxhnNk", "name": "qMvXUVJjim", "points": 26.339436 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KJalQpuAko", "name": "rVXTWekoHW", "points": 54.96757 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qkzgyUoKgF", "name": "AHtfKyvmkW", "points": 51.00772 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AgxDEUmEbF", "name": "KPvjRqLnVP", "points": 38.838684 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "evugRiQYTn", "name": "UXppFajRPp", "points": 53.478706 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "bhQRQjoMtM", "name": "QpaOygbfWV", "points": 29.166164 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "taCOBmTOKx", "name": "DclDNLkYsX", "points": 74.51438 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tgrosmzAot", "name": "FmiOioKRhp", "points": 88.361145 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NYTXfvgFTJ", "name": "iudKFeJgKo", "points": 35.016155 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HLhRWtDEbK", "name": "FFKBQMREGM", "points": 38.041695 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VinOeyNjBh", "name": "cgnuIJYYtK", "points": 61.187313 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "YPCXsnbVfS", "name": "JdVdFfkyBI", "points": 17.272198 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ywUhWtZpxk", "name": "uHdZHlpwJl", "points": 77.51691 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "wfbuzCTJCz", "name": "QyJgUkCTql", "points": 12.335938 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lxRoiFmnLU", "name": "UaJjOuieRV", "points": 85.484695 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KJfJXghsVO", "name": "bwkEwXFDzG", "points": 89.83335 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tWoAaRStrT", "name": "WmmNelqspj", "points": 70.04338 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OnSkcFdtZq", "name": "ohibyKDxjY", "points": 94.26139 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "GXlYgjHKCh", "name": "FKCzGPNdCz", "points": 79.372444 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "tOqBJBQacK", "name": "HRHqRLPBqR", "points": 29.387913 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ZTrjnsukUb", "name": "RAVgpYjXwi", "points": 49.707214 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NtkUjcXoyN", "name": "wuDTOkEPSh", "points": 77.509705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MVhIuyHbCs", "name": "KrnhwWgiwd", "points": 46.75459 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "bUmoYMRFKe", "name": "IObAKPFTgO", "points": 32.610126 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "Onqikdledo", "name": "rJJnqRMziu", "points": 89.72292 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WkFXhliPVT", "name": "xUoiKNEzlP", "points": 33.047714 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "uvWbDwyDSz", "name": "vyUZtEuowJ", "points": 43.62046 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WjKHMShhkT", "name": "ujQjaVpZao", "points": 20.550488 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "kdZTmbCVid", "name": "agfZAVmJFA", "points": 53.971684 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "irzYiHiDji", "name": "UjglqXahav", "points": 26.704731 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PbkAwfnHSl", "name": "mDfHkZNWjR", "points": 83.72709 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HeJvuUNREr", "name": "ArmYDcYBnC", "points": 54.386974 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "srUwWHSiNb", "name": "FCzHDYnrlx", "points": 80.95538 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VaKTFrParZ", "name": "hrdhyQZmLk", "points": 72.157776 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "fladxOSaoE", "name": "YiEZPSAnZB", "points": 86.55242 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QllVPnUyHW", "name": "WVavixbIuZ", "points": 50.739872 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "VLgcyPHZRr", "name": "tfLrXioinB", "points": 28.863749 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jNwfSJHqNl", "name": "WtRrBAZqXE", "points": 83.453804 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ebBqmrkcAb", "name": "ivGVBTNmKK", "points": 5.7974615 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IkbbqGCLwh", "name": "JSDlsfOZgk", "points": 89.774475 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "zgcUqqPBTX", "name": "pnvRMVsHHN", "points": 5.377566 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "HMynOqCIeK", "name": "GgNNQrWwcy", "points": 98.1138 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LMBdDSLFaH", "name": "DqbYSQrsKG", "points": 37.226025 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "jpVfIVBCNO", "name": "xoTEsFERCC", "points": 64.18412 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "NNjBeOIGhq", "name": "LWIxjFZJAV", "points": 13.051935 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "juwPqbLcln", "name": "SqETwSeAbm", "points": 24.913294 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OlNzkmOqFI", "name": "psBSEQKNWR", "points": 76.85818 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rLImHbWKvR", "name": "AIkrfxFNIw", "points": 13.160123 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "gMUYGPXLRO", "name": "ElugAZrztY", "points": 68.86121 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PVtvuadaIt", "name": "pOaqJfKrYp", "points": 74.91805 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WHFJsYfGLF", "name": "cmhdIqtxTS", "points": 74.95362 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OmxOwVoult", "name": "wDEukbLRDM", "points": 96.880165 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "UaaqKlmsfy", "name": "mVQazZwpmA", "points": 97.05421 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QQPYBXSsMG", "name": "BcnlbvBEUm", "points": 5.778454 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "mJhbXHZdCF", "name": "xoxAWRhlgT", "points": 98.51542 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IqfzEwfevH", "name": "aRWemHOEbD", "points": 54.024006 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "TWHAsEnwKz", "name": "QywzjFggoN", "points": 54.17993 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "ojXCjKtliw", "name": "QyKtvRFMhG", "points": 1.1713814 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JVhwzFVlIx", "name": "BewQxZeyFD", "points": 63.61302 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BMMhyUHetl", "name": "XaLuTNUFUz", "points": 44.974197 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CgFIuvAEWj", "name": "ojOtFZSRBP", "points": 74.58219 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CLsiaYmMxS", "name": "eTzGpreyrU", "points": 87.23722 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "AqZokCuBPK", "name": "UWbJlUXesX", "points": 24.558918 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "yKFakYVNKO", "name": "HOXxLDsYOI", "points": 8.326028 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BKPDquWrKQ", "name": "YZNRlKfQbk", "points": 20.743357 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CdafrMeJbE", "name": "YnpjpKJQqm", "points": 5.075859 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OyHHLoiDJC", "name": "sVrLaLaivS", "points": 98.02418 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qatUJJAfnD", "name": "dLBVJFtyUd", "points": 53.793423 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "KsYghiCOHA", "name": "fcQkCixqhF", "points": 23.186323 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WtXRigJtZN", "name": "NRLvCOOwYW", "points": 58.102898 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "oYMuLsyWrf", "name": "glBLTQjlUs", "points": 97.61305 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "lYnRMkxuOp", "name": "ecojrUreeM", "points": 21.188934 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "wRXECwXutz", "name": "tgGCahwJeO", "points": 20.253515 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "LIJxFbIwkH", "name": "zHABFJVprY", "points": 2.9367292 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "BdJkoayaRj", "name": "koxbuugKdk", "points": 13.108878 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "PQjBBSCkHc", "name": "XqhAxIwVLo", "points": 31.030016 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "hymbhlTEkw", "name": "SfEUqSDvzq", "points": 85.49926 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vIvtPfOHpc", "name": "sHQReLarVB", "points": 1.545688 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WaXIXNNdSs", "name": "AgBRBEjHqj", "points": 38.185413 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "WhCGsUUjLP", "name": "AYaDVjvbtG", "points": 29.333773 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "OqsQYGnalh", "name": "IfLXvxzNfz", "points": 84.947914 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nkkTtAjmUY", "name": "VFRjZxJRBX", "points": 73.6919 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "nrbPCyvBAP", "name": "bcosFslITj", "points": 65.17476 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "vBlcLvDeOt", "name": "DoxeaEqGda", "points": 49.4386 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qaRotQTcOh", "name": "bhdQHytkoG", "points": 84.30942 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "dsqirujUjA", "name": "rGPJhoLYqd", "points": 64.26414 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "JMTJBntSag", "name": "kaxUMvdVbH", "points": 27.541136 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "IbKrjNgZsP", "name": "GGVbbSoopn", "points": 49.385166 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "CzkXLzVwFF", "name": "lmCPsPTWhQ", "points": 47.48196 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "MIiwjPsqbG", "name": "ibjULmekdS", "points": 63.392803 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "qtYrcuNcVV", "name": "SQILjkUlQb", "points": 19.790195 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xRJksRCpNQ", "name": "dPMLbIeFCH", "points": 1.644901 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "diMgokEXtf", "name": "awLIoJklCJ", "points": 52.9451 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "DwIMxzYRkJ", "name": "YXWjAQBXEc", "points": 70.59227 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "QqeFZMlQuN", "name": "MrhvkoBGIM", "points": 4.6175385 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_with_float32_actions_2() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "xBMFnDHevK", "name": "VNpeFdGBsz", "points": 54.97176 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "rwUMNdsISf", "name": "fjJKWcMlwn", "points": 32.889374 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "IndexedUser".into(),
            doc_map: Some(
                json!({ "email": "RIAwxhAfTM", "name": "trGYePBrFe", "points": 1.3957142 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CMaoapjjXr", "name": "cDsyxKmVRr", "points": 6.908962 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XhgDCMKXgs", "name": "nnqKWiAPZS", "points": 97.7873 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VTYdGiVpxu", "name": "xPOWtztasJ", "points": 1.2249918 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "zdoVkTRyAL", "name": "RPDWrzXLwe", "points": 95.54296 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YgqaumCYAP", "name": "JCPfGBYict", "points": 29.657578 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZDnUcuENgM", "name": "qSdHSwYYXt", "points": 92.70995 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jUNFwbkUKJ", "name": "kRMWxgrvzr", "points": 48.2015 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pLvUKOqWNl", "name": "ETEdRelCaH", "points": 40.729343 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BrauNJPwfS", "name": "qPDVdhYzYY", "points": 53.959274 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pYbVNrdjUU", "name": "oiIZWySrvk", "points": 40.380787 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZJfchNUvyl", "name": "VTlPbGZKdo", "points": 42.801304 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KmvUbGEsAN", "name": "QJeKvQuKrA", "points": 37.401924 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bBwyHtWeqU", "name": "ldhXpjcpos", "points": 74.58328 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gBepYRlYBt", "name": "ryXtIsffcp", "points": 31.810175 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LjuBfszTmG", "name": "nUTQOqDsQL", "points": 33.61086 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ohswbNJZaB", "name": "AhvXomHiUp", "points": 55.44609 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rlGpupaYRU", "name": "HyKkEaPqrN", "points": 6.801659 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RrSPOGfeKr", "name": "gZdBmYZsWE", "points": 94.8964 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mwZIjuXDps", "name": "NvHchKPmsV", "points": 4.154451 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uSaesaszWW", "name": "NHenqFbOgd", "points": 49.565052 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aiFmcfseWO", "name": "ONppyRpXjX", "points": 46.497242 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AJcOALLTXI", "name": "ZcNHzVPdIX", "points": 8.243556 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZLWMRGsCYJ", "name": "CwlkCFvZvn", "points": 78.57368 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "osWmnmQByj", "name": "jBbzNSCuat", "points": 76.701004 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VBSuCyqDzL", "name": "pThAzqZoRS", "points": 76.52672 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gJwMukqSdl", "name": "sarNtrKDwQ", "points": 20.968317 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TJrTFxHkwJ", "name": "iDfxRUyhoH", "points": 85.42814 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ixiFxbrBrs", "name": "WFPTdCaFZC", "points": 66.75774 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rgQDeveMuv", "name": "hHcosDmnQu", "points": 58.910114 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TciFfzApwZ", "name": "BzixQVHkNn", "points": 22.249302 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YVuebgNwXF", "name": "ZlalMqBNqb", "points": 66.23493 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lbhtHKWXMi", "name": "eraBAnKyEX", "points": 23.969658 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lNlALyuNNn", "name": "SgXoQSxuvc", "points": 35.987915 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rKCahFSThT", "name": "fSytwCrTYX", "points": 31.335897 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pffLeyRGQc", "name": "FJGexUowLh", "points": 35.517147 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oqspEBdlKB", "name": "oUJSUCxqvf", "points": 87.33194 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KmGDEjDocc", "name": "NFTgZWCbGV", "points": 77.38898 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VWhdrwvXQc", "name": "TqkHarXPth", "points": 41.174496 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hELGaVlMTV", "name": "DaPlmIIOLr", "points": 54.3224 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AnyiATgYUe", "name": "VUIdRgeXUT", "points": 75.99071 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DsLlDVuoKb", "name": "bpNlxPTHtg", "points": 47.422787 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OgZXbbvGFb", "name": "oFUZWuBGEA", "points": 54.01121 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YUhYrZDlNy", "name": "bSsrybKAnZ", "points": 69.40306 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VwzxACaFkc", "name": "ScOCpIsyKS", "points": 55.597702 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DxLgSlWbnq", "name": "bQRpJwFpkQ", "points": 78.0237 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tFhsMwmjXM", "name": "DSeoGiPiww", "points": 71.07976 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jAqjOKflPS", "name": "jVKUYUPxqL", "points": 74.83074 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pqRfEcgZgy", "name": "mMNrnIGUnB", "points": 80.62306 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pXmpzDChaj", "name": "VhCxWqDWBf", "points": 76.432884 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RblkyPbdts", "name": "FwrXoToZiX", "points": 76.0098 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AKHOcqsGre", "name": "HYKDrWGNoJ", "points": 45.146515 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "eTDOVxLYXr", "name": "OMssjIFELG", "points": 75.35859 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xzouquEJFG", "name": "XWvyVAeruq", "points": 90.947365 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YMPlTzvBCV", "name": "jPOeZdLAFm", "points": 84.351555 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MvuJTvytAc", "name": "htorjYYZUV", "points": 11.841045 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RNMvYMaxxD", "name": "gpkmyjAJjh", "points": 93.8555 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dDzyudUXhj", "name": "kyReSPTDok", "points": 44.286457 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tbQCXXTZuA", "name": "efjTyKXKeK", "points": 23.427458 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CkmROoILyN", "name": "nqwkPsfYFX", "points": 44.09772 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fErqCIdtOe", "name": "LhZkzIHaeW", "points": 75.16791 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wJarACvhks", "name": "nPPgXJltSo", "points": 35.390476 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "caYOTSRuTR", "name": "vYUpuLABqY", "points": 21.041443 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ubVPTJKXaN", "name": "eQqbhMOynn", "points": 36.56304 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ivoPwWLlWq", "name": "PEJZGtrUFX", "points": 83.4336 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YjiSZJXzON", "name": "aFnVjAaJwS", "points": 48.15735 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FFSfvljotN", "name": "sdvuadnOPO", "points": 30.24637 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "frhHpZNgXm", "name": "VZOvWTGktW", "points": 71.55303 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ybWFehToFE", "name": "XgcZSpmSvw", "points": 29.680603 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SVOpemwCty", "name": "PVbUegvDtK", "points": 23.212929 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "qrFWqUgZLz", "name": "XmRTnZmJXA", "points": 22.939615 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tAtDsfbakf", "name": "AegQCTeMzk", "points": 30.330976 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lKDRscXkAG", "name": "GHUOAuVXAS", "points": 44.01879 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hEuveoQvHh", "name": "cKQvuJTTBb", "points": 19.417603 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "irvDZDIami", "name": "GHzfeYYjlk", "points": 40.303684 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jsmlfcOIZK", "name": "znAaLvXXZK", "points": 6.3565116 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mNJzZxcsMJ", "name": "rmoWyLHjqe", "points": 72.14149 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CdSteVRKjE", "name": "CxyIPPhXPm", "points": 13.000809 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lyHXdQYllx", "name": "eEkNoZCrqK", "points": 71.26355 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tPTVYReiYM", "name": "mClopdwLvo", "points": 12.22026 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pgMXjifMgM", "name": "FDgUfGwTpC", "points": 24.903254 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DsSdrHRhur", "name": "SbmjxUXzov", "points": 24.101225 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hSGmlWbLSv", "name": "ltzujZswGk", "points": 80.57161 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gezYMXJwUR", "name": "oqQzBzuyMO", "points": 68.860016 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pjnZszBEsb", "name": "rYqrcuYIgc", "points": 1.2962406 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "zgegUcvpiy", "name": "swVzDefeTx", "points": 25.301126 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HcqaYNAFRh", "name": "vrlknOiFeX", "points": 7.0701795 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BwUcufFcSP", "name": "QdYrElcvHM", "points": 19.43735 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wjbbqpPwOC", "name": "yaqVttDweP", "points": 91.10934 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "geOKEQQwxs", "name": "qxqgfooITd", "points": 48.481323 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CWVWzFEcYW", "name": "TgYTaLZmMp", "points": 19.118176 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jnBPUNucHv", "name": "YUgaPAvpYG", "points": 45.306343 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QhMhJPfWFf", "name": "DEsDGdJZhZ", "points": 1.1624582 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "EyiwxtcdWS", "name": "RDoPZcEtfA", "points": 84.39299 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oktophhVWy", "name": "IWwmyftrtD", "points": 68.153336 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aYExWNXbka", "name": "iOAyZOhkrq", "points": 87.39678 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ooqXAdGkQt", "name": "FnptKmNulP", "points": 60.848255 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OrUJOphjSJ", "name": "uXKGonVfBx", "points": 79.98937 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oYjtqEqOXt", "name": "WqaiZkyrEO", "points": 93.694435 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OBGUHolHrt", "name": "nNykcafHrT", "points": 54.654118 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UBOKvfQitw", "name": "yTAlJYeiXA", "points": 35.57709 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BDVeozSEWk", "name": "tOtqoTFpHX", "points": 93.14117 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AbSAWOagcF", "name": "hSKrZhPJYO", "points": 6.8756065 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AcxbihQMEE", "name": "slFSBxlwoa", "points": 33.28884 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vajtlwVpJJ", "name": "bxSwMZWZiT", "points": 45.60721 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bjMELhRhWr", "name": "uAspnPZGLB", "points": 62.058647 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZSmTpelHaR", "name": "qNGFgZtGSM", "points": 52.323284 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ViwHQUtsDd", "name": "umLlDxlEcW", "points": 51.260563 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TUCRbVyNZC", "name": "tcrdvAfbNh", "points": 74.685356 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "COAfFQNyYW", "name": "MiuBhfdyDt", "points": 89.1223 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GWlThxdcmL", "name": "HJicOokdxE", "points": 90.87186 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xkWRURiLKN", "name": "vnkirGGBDV", "points": 31.625467 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "nzVSqmIaGm", "name": "enamxauwBj", "points": 23.567677 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KdhLXmKLLu", "name": "OkDhjjMMCa", "points": 96.85345 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GJQgJraEpi", "name": "pQWutzyMpg", "points": 92.23045 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QbtoxVuPyW", "name": "sFAIKoUHfp", "points": 51.713123 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uHSgNklCXH", "name": "TDkFDTuKfY", "points": 94.02875 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YEzJbuJRhl", "name": "UsNsRwbryC", "points": 37.81436 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QTqXefRcmO", "name": "KjyDBZvJYG", "points": 92.17938 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wOMyvIjgUX", "name": "oHirIOgxFx", "points": 41.525646 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YzTukzCOeA", "name": "AnAnYpMMNN", "points": 75.25447 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ibNGgtdTeh", "name": "nFzJGbmbgn", "points": 82.04186 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KkXzjvlrPb", "name": "CYVDLaKZGH", "points": 46.760757 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CldaotCBUk", "name": "dWiiJiDGAg", "points": 14.83336 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DkTFSvVovd", "name": "yLkybRYuUL", "points": 44.36417 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fwvyzxyumA", "name": "ViKaNuilpA", "points": 48.701187 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "EoxbGXevfU", "name": "ztuufQnjFQ", "points": 35.122887 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rcInaIAYlb", "name": "RZxFtQcdRU", "points": 27.685614 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZWbekfMjLl", "name": "trDIPWNBLM", "points": 86.543816 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jrnODVgZwB", "name": "tqvfjDUlfA", "points": 37.460808 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UJNLTBGEca", "name": "PIcNxoIRtc", "points": 48.806126 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lunneNTHWi", "name": "XdynTltyHI", "points": 10.788356 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vPxilvqKOj", "name": "UbJqvMXZLv", "points": 12.581716 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JTfsIOlZfX", "name": "UmPPVQUiOr", "points": 82.00173 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UhapzdZMyk", "name": "GxMyGIAeDO", "points": 92.37376 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "zdOfuFmKgP", "name": "jgbffjDnKf", "points": 9.209242 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZSwxkLfFpx", "name": "qJLhxoGfsC", "points": 70.4311 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fbUcqjVRqg", "name": "PriYgTLCrn", "points": 55.950584 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "heCoPVwSAk", "name": "plugsvngvJ", "points": 67.73612 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "shFbmarJog", "name": "jwilpULOAn", "points": 34.686386 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oUfHLtloHo", "name": "cJrOaUGYaK", "points": 86.43927 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UrTrSIUvra", "name": "VgCJMUQmex", "points": 12.863526 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TFNbQHtWlM", "name": "zxEoVqwNOI", "points": 89.35739 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "msZcUfyoUt", "name": "odvmyDAXSv", "points": 62.347836 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "glmjaDTgQp", "name": "yMieNvzqld", "points": 14.415842 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oWktYXnsHo", "name": "GHPjQxdYDt", "points": 17.983328 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jeJpsiVkBq", "name": "IgHmyitKSh", "points": 4.001645 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NeGXSfmBLV", "name": "GCQZDTanyl", "points": 42.50904 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OejNfMIIXw", "name": "rZmTNGLLbF", "points": 23.754433 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uPjXNbjfPL", "name": "tzmwjbWuUL", "points": 35.601143 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vkCYtEaADo", "name": "oaPCUxurPA", "points": 11.198496 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GELKDOOUFi", "name": "kqHJduwqNl", "points": 21.39663 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YKwuOemTTF", "name": "rgRtvdNVuQ", "points": 30.625065 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JbYcHIirfC", "name": "OishAQclVP", "points": 73.72819 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RPVOTcJxmK", "name": "igpFwIfNqb", "points": 28.361258 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "riVZVEbRmf", "name": "TumHjzDDva", "points": 63.197083 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xhZLXGJqLs", "name": "MkPFBGTuTW", "points": 43.042118 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KpxQMMErjH", "name": "DuADdyMFSm", "points": 84.31566 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dLqQfzJwbE", "name": "bNYMKkKazQ", "points": 95.915184 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "yQDQBMYNqY", "name": "KTHcgaMpAU", "points": 38.452892 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DsnLnNKlJZ", "name": "fQJkzUIVwH", "points": 9.182382 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NnXiYxxtRy", "name": "DpHGmiZgVp", "points": 88.53165 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AiRoBaRFmX", "name": "jpJndLKLEF", "points": 46.324154 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YcEctqaKLn", "name": "DbXMvBXgfC", "points": 80.14113 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vKISrzwKqQ", "name": "OYpSiIgvOM", "points": 92.45759 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uiGHUlBrkp", "name": "FopHEigxJy", "points": 26.64689 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "feQNebzaww", "name": "yIyDOVtbhy", "points": 90.99367 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uFmmpxLIey", "name": "COkbKrBwyV", "points": 58.623173 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ykiCmnpAvd", "name": "bGeBxoFtqQ", "points": 41.481766 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DPmHLUThqD", "name": "dAsAJgUGUR", "points": 9.609783 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "EjSyjKyUfp", "name": "HFOxULtcDI", "points": 18.998337 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RoIxzvwKcc", "name": "iEUEJYzlgc", "points": 57.56013 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xvayZbSghE", "name": "IgfFizEmHb", "points": 61.774864 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PdnphvBdJL", "name": "PxZFAkUYbx", "points": 94.39505 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ziVImrfIZI", "name": "tayFpgecVa", "points": 14.335639 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jZjxwNsVVd", "name": "sehYwXfvjV", "points": 93.000984 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "EawhitMGUQ", "name": "VCBGvPYULS", "points": 27.088488 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gyqEZKtIkP", "name": "aREaUVKDph", "points": 24.331009 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DIulFTTcDq", "name": "SnylcDCKav", "points": 2.556123 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IdMsGBERnY", "name": "olVaimLeEf", "points": 56.45753 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HSPDFrqsRO", "name": "MxnYIHojTU", "points": 87.28739 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VgWGzhjfpk", "name": "vCcEkLZkwb", "points": 76.16705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "duURUOAlHL", "name": "XIuTLsQwtk", "points": 0.726452 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bzLcuKnFnw", "name": "zNkICaJGCZ", "points": 40.58098 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "email": "NoDEAPJsVZ", "name": "vmGeOewlPz", "points": 96.844 })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZgYeIUgavq", "name": "ZojmcUxjPL", "points": 37.26256 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PUCcnpKLOI", "name": "paFensoNzB", "points": 87.61084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QYbixOInbX", "name": "spLsaIESFb", "points": 83.871506 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "prnjXAlwhX", "name": "ChQALnFpiy", "points": 45.059628 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SLKwThvFZj", "name": "hNDcGKXdgF", "points": 96.780174 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GjQbweMhOf", "name": "kaRiTEhumj", "points": 10.585125 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vWVlZzIBoi", "name": "edeyophSbt", "points": 45.134705 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mHIkHmSesB", "name": "lYqeItsGeB", "points": 24.964596 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SKhccEjLFN", "name": "bqYiPEFWHo", "points": 55.429874 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BNtSDaVWwa", "name": "mLGpSKlkfh", "points": 54.531498 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TiAStqXHgu", "name": "wmjOHWYxzE", "points": 68.87882 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oMDAgUrdsl", "name": "JOxZdYKCkZ", "points": 82.24078 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xsQSQpGvzi", "name": "zpFUuxNDwc", "points": 80.93224 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OpKACDwEyR", "name": "rqutWBsaoR", "points": 69.503204 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HsMoMjsbwn", "name": "GybrjuRvfU", "points": 20.9503 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bYdgUaLSjB", "name": "OUWqbUKUHb", "points": 7.7595553 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pPmNrHtaPG", "name": "kpZapzOqVQ", "points": 57.341393 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gattWlHTrG", "name": "JVdPAlAAbf", "points": 45.48825 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CRcVeMMrhI", "name": "lhhRzGdXnt", "points": 35.254917 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "htUZsvDFHm", "name": "mDBwtKqNhA", "points": 1.6209671 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "kqsgWuNxQe", "name": "pgoTHndyNk", "points": 8.179446 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KpINvmtBih", "name": "GqSebhwdmu", "points": 83.80934 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xAZoZcbnpQ", "name": "LWkTvvmGBP", "points": 39.172375 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hcJtYiMiui", "name": "TnUbUaCDLO", "points": 33.04843 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FRjuZniuGJ", "name": "yogHUiDYEU", "points": 6.4297633 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uYSBBuxiUK", "name": "BwPGujjFfH", "points": 18.976215 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "kgEpFOeqYA", "name": "okInNAoleT", "points": 0.9659261 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tvJYZPiZxD", "name": "sIETuDVWrH", "points": 10.233404 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FCEVfsjSEp", "name": "uZLWyHbWsa", "points": 89.97464 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PUxjyHqTPU", "name": "tdbxpIEEXF", "points": 71.988686 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IEHIjyvBji", "name": "pZThrYBcIx", "points": 7.8132033 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "facJoaNobX", "name": "WAKAdyIZHk", "points": 44.417645 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uftcgSKekV", "name": "ceOJBabNFt", "points": 73.69564 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xNmoUCNGYu", "name": "SjVghTkGJQ", "points": 32.1906 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BSftiVhiVJ", "name": "xznIjelPAr", "points": 27.63078 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VMANHgqvVv", "name": "rvziCPQCCD", "points": 61.249084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PPThRPHKhg", "name": "jsCWmjiGHj", "points": 4.906907 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pUlYiAqJHZ", "name": "xxxlzGlngY", "points": 2.6626914 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "psYCxSUKhw", "name": "KMkriuRpTB", "points": 86.10242 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LAAiCYkmuN", "name": "yYRknXprAJ", "points": 92.06586 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WVPTquckhU", "name": "QHHSfHIWqH", "points": 25.76062 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YWWMoqkaGF", "name": "BTfulNyYvP", "points": 20.00514 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bWBMxOPLIl", "name": "qwYBjkSwKO", "points": 5.865848 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XvVSCzurPH", "name": "rTcCjWGXHy", "points": 58.65999 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LIXKCSzQQS", "name": "utMWptRawb", "points": 88.13121 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ivLlbvAnWM", "name": "EGeKpSWDIb", "points": 69.38865 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QhjABvGPjp", "name": "yBYTHulMOU", "points": 69.84171 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gOLgxnRRgR", "name": "hTNoPcbiOz", "points": 91.51237 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "roSLNLXkhY", "name": "uRTvuDXdCq", "points": 68.59787 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "iWQeEOxPZO", "name": "aOMMpjVQEv", "points": 4.4774823 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YjjkdcaQQS", "name": "pdbVylZjSw", "points": 13.19637 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QGzhyfLcuV", "name": "UFFovDVGmk", "points": 43.971073 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XwzSRhguoC", "name": "UBMMrADISR", "points": 96.4793 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HqqIbOJUii", "name": "ypCCIFntvb", "points": 7.3566017 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QayJnafKKc", "name": "TccEWtDGTZ", "points": 82.906296 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "iOgBBGNOXY", "name": "AmobaXXRXh", "points": 52.20084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GWVjmbSUip", "name": "VNxHHTYhUF", "points": 44.93017 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pfEFLnGINR", "name": "fpMDrpkDTR", "points": 87.58644 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FFLTrQRKRQ", "name": "GdVtUxwLel", "points": 61.21079 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "assZzyPYzR", "name": "rIpSLYvLua", "points": 65.23396 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "iaagNdSqET", "name": "tttpxbIsVv", "points": 36.952774 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KSAfasJHMF", "name": "jduXwrDYoN", "points": 56.04759 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WkKHTXXjtA", "name": "VwTFbQhGmX", "points": 86.114265 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_with_float32_actions_3() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FDUINNVbQT", "name": "folCJaABBU", "points": 10.811715 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "qWJnOdHyMq", "name": "uqKelHxkPC", "points": 54.451347 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RESjySQdZx", "name": "xaBWbhluVg", "points": 77.410385 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TQaLTyTvFG", "name": "ycDiwuVsPq", "points": 81.52999 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "qexChnPIuP", "name": "etkAqIZJan", "points": 10.72661 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IWlAnTKBXy", "name": "sZdAqRZByG", "points": 38.896706 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YrOERmBOAz", "name": "fheoprQqAD", "points": 22.871572 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NSuVWPnoon", "name": "NKArrQisFu", "points": 23.46791 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IsijICAyhO", "name": "gEffNvhAuL", "points": 12.847034 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VlxbTPghsL", "name": "WNbPrxdkBL", "points": 91.401276 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HmDBnwljYn", "name": "IRJkpbHVSg", "points": 46.510227 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XddAchGuql", "name": "POmsVwlZfg", "points": 55.121513 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HKNEzNhbRb", "name": "TqdBneWdjF", "points": 78.67752 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rJrxEgbTXN", "name": "TfaELLQafl", "points": 6.526362 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LHwAoPqZgK", "name": "PMfjLnZdIK", "points": 69.34504 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jkYYWkrSaN", "name": "LoRfzzrQPe", "points": 43.98622 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ECepUQJPxm", "name": "AAcuwEOrLg", "points": 66.77671 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QlWjTqLERv", "name": "mjhGlpOwkk", "points": 31.419533 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JqckWQAGSC", "name": "fLuDTcSURn", "points": 27.894451 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RBrscIJzZO", "name": "HJNUDLmrGW", "points": 69.11451 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mdIWEczqKp", "name": "PQwmxZbUZD", "points": 57.084286 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PvlQYSxwIb", "name": "WiIdmnfkIU", "points": 47.267574 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "sulcomoYPy", "name": "tvIkSgYQkD", "points": 12.14582 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ORRqTeYCeC", "name": "uvCAcFcNkn", "points": 78.96998 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JyXrTRxkyR", "name": "LJgrriRNdg", "points": 76.97646 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CmPdRnGTKK", "name": "LLOzJQTGlO", "points": 47.92818 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UseBxxipTI", "name": "muaGVdnYrW", "points": 36.46769 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mMwvAxFESp", "name": "MmdcolQRpj", "points": 3.3829837 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vRRVTtLUxk", "name": "hwTWGIEpeK", "points": 11.9373255 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ylRdQSzJCA", "name": "odMvyfRlVZ", "points": 85.56568 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "banncOZwNn", "name": "WPVnIbDYMD", "points": 28.51936 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dAdcovcdRD", "name": "iHYnHwqbFI", "points": 48.1692 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RnlRVdpIZt", "name": "NSLRaebjIn", "points": 95.63688 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KFQjVLvHOk", "name": "CqNwwkDJUO", "points": 31.472776 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ayzlkuvUCa", "name": "ClYVNqgsSS", "points": 78.68829 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hAcoQDryUF", "name": "ZJABJwaWTr", "points": 92.57055 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jppgItPQRO", "name": "UpTTkzjKAW", "points": 47.1519 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RIGvuRhZhw", "name": "tPvNzkOzxc", "points": 47.515057 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZnlSYWAEHy", "name": "NAIXoDurVY", "points": 96.656746 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZwUYkGgCBA", "name": "xFqkCZfGwU", "points": 74.957634 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ROaCJSVQIF", "name": "YEYzcNyYhR", "points": 71.820335 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jwTkDveWTd", "name": "MXICfMpwhE", "points": 6.0639668 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "sJXLuoZYYF", "name": "QkZgoIywVK", "points": 10.28122 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ydeEtNIIDC", "name": "DZHBkfNJjM", "points": 46.061726 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "cyCnGnERDu", "name": "ZpxNBOfIDP", "points": 55.647297 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "yCfIhvoUAw", "name": "rOhvqXpSSF", "points": 84.72746 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jrlbHpmFau", "name": "KWAnNHqTOZ", "points": 43.31525 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FJXbmCPZko", "name": "WPuXGlXiwl", "points": 4.95704 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aTHXzdSOCQ", "name": "NgFnVqjTbf", "points": 24.513273 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SlwiilzHjE", "name": "TBSulVjPgM", "points": 93.345985 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dagouosCeg", "name": "bvHaguYtNv", "points": 31.09748 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NpzHPVqECv", "name": "XKAGPVgwLM", "points": 66.02672 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QNNciLMHWy", "name": "ZHsrFoqUIf", "points": 14.030453 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PHUWJEWHtp", "name": "GyqjwBCesV", "points": 82.98252 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YznajQrHFO", "name": "AZUtYsBlpy", "points": 88.54152 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xluieDTjMZ", "name": "RsVLdirVqW", "points": 34.25893 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wyFcYUBYRO", "name": "GDxpDyFvRi", "points": 12.349107 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vWYiTKcEmQ", "name": "umeYfGjVGo", "points": 3.9236503 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RhyBHeQuqz", "name": "eMicJikSwj", "points": 45.843075 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YgnsHnaTyM", "name": "FkKRSFcZld", "points": 74.88801 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JomxoijCsy", "name": "xEDWWnDRoF", "points": 38.6253 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mLCCqLSJRl", "name": "bXGTzASIPr", "points": 13.958686 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "kXsUDvPSLx", "name": "iApFPFGDJM", "points": 33.768436 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "emqksKVfGr", "name": "BSvqPMsOWe", "points": 32.812653 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rgRkgWlMCG", "name": "ZmkcQSapVW", "points": 84.667206 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IwwXzBgXRN", "name": "TAfsqAtQOz", "points": 77.26203 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XdRqtgdGoG", "name": "clrtMCaJOx", "points": 32.57516 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "kUlXmvYHUR", "name": "jyjyVmZLJy", "points": 47.417744 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TLFrkHvJkF", "name": "KCynjONrmq", "points": 44.27111 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xDQwvKjEPV", "name": "FdcEbYMOkU", "points": 80.87933 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "phBgAHVjZp", "name": "FrkKvdTzVG", "points": 67.986916 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PTuriFqChK", "name": "FSYmihIDMH", "points": 77.214294 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dLjIqWfvbL", "name": "moUWGRmMUg", "points": 50.266006 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lBTnSaMyUO", "name": "UTeLDxIGld", "points": 76.67837 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UVKAFhVfjZ", "name": "jUWQOVKoqc", "points": 34.554897 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XYCLsaQPvu", "name": "IOcmfaWfCs", "points": 82.66117 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "cQCpJKftfI", "name": "xsEBMUPBIf", "points": 71.03898 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YFTPZmpDRN", "name": "FbftZMDvjr", "points": 73.45413 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fFgLBLpemR", "name": "MVqocpADNe", "points": 85.64454 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dTlgLJJMqb", "name": "mlaqLxpehk", "points": 20.659704 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ldCuDBActe", "name": "JeYuEIKyxQ", "points": 24.014664 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HNpcXoTJjm", "name": "dvFdcIXxjg", "points": 53.937878 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UczbrrfvYt", "name": "xOyIEFfKrT", "points": 69.41057 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "eJLXRQdYbp", "name": "AKKcyQKzfF", "points": 61.57481 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MEyVDOFCZc", "name": "BgcvAFZcnF", "points": 63.89523 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "kOJCtxykIR", "name": "mfxXlmdVzC", "points": 87.74736 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jmoXVgAppa", "name": "ZSMTNspwux", "points": 89.8195 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UYTEXelhQL", "name": "TXGZBpJyvb", "points": 41.333622 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NrLZVUoBDh", "name": "tFxuKaijyD", "points": 83.838524 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pntySPZSCn", "name": "bocqRoFhiT", "points": 68.58467 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dKkctWhqSm", "name": "NVFvtvVpFd", "points": 34.47484 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZzKhqEaCvr", "name": "EcHvIvjSqj", "points": 19.065474 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WGfRiOSGtw", "name": "kdQEFlOLCe", "points": 82.4626 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "doJrOncltS", "name": "whqngAGvZb", "points": 24.399431 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FnrgdOmZFp", "name": "KOWEvbLzrF", "points": 44.952866 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LsSJXzucaE", "name": "PKNkhowhoF", "points": 47.934452 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SGzUQZXrkN", "name": "JiGRyBuXoy", "points": 93.88255 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TxkDBZPJJh", "name": "CVlLPzOUCF", "points": 49.192112 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bMfmDEQHPU", "name": "pKzVYaQrYt", "points": 78.81753 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KsrgspzgUT", "name": "jeJWjSNFmO", "points": 31.996828 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QqDkQGttvg", "name": "RkqJkuvHcG", "points": 50.229134 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WZExCpSPsY", "name": "QedlyoFRvO", "points": 49.884117 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KdmahQUGxJ", "name": "IBGOAwRNJp", "points": 28.289713 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OkPkoErpDh", "name": "lMWqellrVb", "points": 8.260824 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BMEiIWYXeH", "name": "jQpYhAoMCm", "points": 26.416769 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aWPdPKzpRO", "name": "OhezvNYsRd", "points": 40.329845 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WFkfeswYAU", "name": "BBSBKysoQg", "points": 52.53978 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MZSSVTzsUN", "name": "aFAUVFbDzl", "points": 19.729908 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "yygtjmmGns", "name": "saaHYVfrAt", "points": 60.60391 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "vbdwTbTwwS", "name": "kfkcKZRqkI", "points": 90.89393 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LAituJByof", "name": "JZnupWAbbz", "points": 22.271616 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "dqZgIaOCnl", "name": "psGdFnJMay", "points": 85.38972 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XRjXwQBvWE", "name": "wJiMUruktc", "points": 56.538204 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QNhAwisYzO", "name": "UhsQLYlPlR", "points": 12.909566 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "idXVbbBGFd", "name": "oDoadLeVMA", "points": 71.89012 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BDJhHHLjZB", "name": "uvWhsTlkVj", "points": 77.82656 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jrpQAnJDwH", "name": "SnxhplHVPP", "points": 91.933136 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PoAczxCHxn", "name": "GkBApmNpbd", "points": 13.111532 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uvIvPeEfvd", "name": "syWpbUhmRg", "points": 87.602295 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IIDDOIoNQQ", "name": "JWmyFNArhI", "points": 52.308773 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YhLZWwUKaP", "name": "kCLpfZsNUh", "points": 73.63633 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ERoJcTFcxY", "name": "iFGvNjHXvY", "points": 42.05051 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GURmyOPoua", "name": "csTCrWtsCn", "points": 31.781284 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FjOKjJIOYU", "name": "iglwbvtiEM", "points": 9.521613 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "yayGEEPJFV", "name": "cDpUbuYszT", "points": 33.793182 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xMBWWhoLGp", "name": "dAiPQRIllV", "points": 57.287304 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bSkANuEfbK", "name": "fTKpATNWYv", "points": 22.713072 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "nqczSkTdUO", "name": "qzOCUdEpok", "points": 49.856094 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "SKcZJOidLG", "name": "eTHBfaZYBZ", "points": 46.304974 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NegwwHflou", "name": "LykCLvlCYo", "points": 87.26657 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "toEUsPxFPU", "name": "lwZPwdiGcp", "points": 77.016235 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GwPNKLDOdh", "name": "BPxMujtBRx", "points": 41.736423 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "auuiHYSUbe", "name": "hPSJtfvsxM", "points": 17.86675 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fSWVxVYTjG", "name": "bBSstLUBGi", "points": 41.299084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VxeuCKZRNg", "name": "nGDioBzaVw", "points": 59.688354 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tayFUNSLyd", "name": "VvEbmkMabB", "points": 86.57048 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZDJEJcCdBb", "name": "JahDFBTteN", "points": 4.965748 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "EtIuslFEYx", "name": "FMoPJoynAo", "points": 6.55779 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BKuzQgnEsu", "name": "PCTPHiSTuj", "points": 53.115376 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LmFkRSTNLK", "name": "VQaNCwrYRy", "points": 31.507372 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "OYnbHNJoMJ", "name": "yeWQgQgpAK", "points": 0.0043809805 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bjaPlMfjhp", "name": "OhBvAlVbkQ", "points": 76.72786 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xEChPFEaiC", "name": "eoWcyMxuVw", "points": 2.277679 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TLGVvYdIdx", "name": "AYdAAZxgRR", "points": 33.324844 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "RfxuvxDSZe", "name": "xyrhFuonnG", "points": 80.30322 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pkxvrdQews", "name": "hzhxbCNKGB", "points": 81.63485 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FVvVCCYhrQ", "name": "CbhEIDWhHC", "points": 24.395746 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DuUbIruwgg", "name": "MWgFvOTJIf", "points": 13.677324 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "zDOojAIehC", "name": "JTLqMeTJac", "points": 61.491047 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LehaDofSnE", "name": "uROezppcJU", "points": 93.346504 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "uEmPYQECJE", "name": "NiEArlvlSx", "points": 63.24658 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UWToFXIgvo", "name": "JHNoDcvJQO", "points": 33.227715 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "zXrvzjRHTQ", "name": "hnDLWFyKcy", "points": 57.359768 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ngRGYxoLPO", "name": "OZAkGKGisi", "points": 95.055084 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "fHWzcynaMs", "name": "XXrniwhzSt", "points": 89.18983 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IeSCtnGKQv", "name": "rDYVOJXrEl", "points": 86.66515 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WlyIoEzZVz", "name": "RBzDHQoTZl", "points": 30.066961 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wegjDcoaFY", "name": "fawihVsndV", "points": 19.319012 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XHTFwpfyhQ", "name": "rDvvPfLAOx", "points": 3.368873 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MPpmZiNDDP", "name": "esEItkBxLL", "points": 13.80772 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tigNmIZgHW", "name": "LlAPOSKgGg", "points": 52.687298 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TowODXKiRT", "name": "CDfXKZqvoh", "points": 68.28747 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HaFWeiobpH", "name": "mMhsVRWMoQ", "points": 72.382675 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bBqcXjUIzl", "name": "qSScsOrIpQ", "points": 41.83511 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UdpDyIgLSK", "name": "PkZbpdZlOO", "points": 2.8834388 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LdaZEmVkes", "name": "QsjnPACyod", "points": 51.745956 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xBjtDDPjWB", "name": "FqjaGXgZOl", "points": 55.804993 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "AJpFREGXCV", "name": "RpBXDSdIbe", "points": 76.12005 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "PRJuOaTWom", "name": "TrAsgHPsEZ", "points": 40.218037 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jOHwXumIuE", "name": "prGWNfOEAP", "points": 5.472432 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "wvUxJmXSuf", "name": "jvyVcyCFHs", "points": 75.46584 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "crIlDpPaOH", "name": "QGTiObGyDr", "points": 5.0142813 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "snLNFttbGp", "name": "iTHjQTtQwf", "points": 67.31377 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "sfeMPcZkHu", "name": "dVgLEfzpyX", "points": 4.127209 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "rmNuubgcZt", "name": "KDIAyJXXso", "points": 39.073757 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FxJzERituJ", "name": "WqsVuYERot", "points": 36.05472 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WCYuUGCrmK", "name": "xanRlyCqVq", "points": 24.652592 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "lTzFIMQQCD", "name": "BAofKpaPwb", "points": 21.82528 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TGgQKHjOyM", "name": "ExkCHDNsUa", "points": 93.133575 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JTJkKfbOJl", "name": "LRcmcLZxGI", "points": 37.491802 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VzwCzmDlNe", "name": "EfGaBCTZxE", "points": 65.6968 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hOLfGjpovb", "name": "xFYzEUtRgP", "points": 77.820015 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CvGvOgDYKf", "name": "QdmGuQJMpV", "points": 62.78067 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VwzVyPKMsF", "name": "dljUkYXYCa", "points": 59.3764 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UTEaLBFwfl", "name": "qCHRotlmjn", "points": 92.46695 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "tWBrfsQAZQ", "name": "kZoZGMUcRB", "points": 55.49197 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "bQIOFcFKoW", "name": "AFoGXVDluV", "points": 93.540794 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MRMJbzQvTy", "name": "HYxFjMnWKa", "points": 32.25856 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VKIjUWMLzb", "name": "AALIVXRCno", "points": 13.577864 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FSlcToMnXy", "name": "YrElyhiwxM", "points": 63.414925 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HkECsxkJfJ", "name": "GyYebKQWuk", "points": 4.76436 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aekAAIyvLy", "name": "oPgWEqbKXb", "points": 26.828232 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aWjuEqABae", "name": "EUXbuWQoaD", "points": 72.55752 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ldwNiYvxDp", "name": "ggWGApkpxR", "points": 92.82106 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hOpsduujlW", "name": "iSOagVIwcm", "points": 74.9745 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MqMZfRzzqw", "name": "CeTgcQaArX", "points": 1.78948 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NbqceMPjLV", "name": "hiNpniZdMs", "points": 24.910406 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DQtDFaTkYf", "name": "swNzFnsOrx", "points": 18.24907 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DUyLTCkHpK", "name": "QxcHvqXSXA", "points": 22.865652 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "VnTDDwCsas", "name": "ABqHbAlExX", "points": 6.8956184 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XcpARbEcJf", "name": "dgOiQktoNV", "points": 6.4051185 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GxoCPYsajx", "name": "KJeBQOoHTj", "points": 33.959175 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JECeTzujbt", "name": "HdjqYwjWkF", "points": 63.559414 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WEBgbAjFrm", "name": "RYwqAjLNsP", "points": 47.454758 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JDrTJEvGHP", "name": "HnBwKiqGAz", "points": 61.208603 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZSdChHomae", "name": "unLcWmngYO", "points": 94.80166 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mlfcBPADGQ", "name": "uOiVAGGqCm", "points": 5.5131927 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ngCBnKuVsW", "name": "tXwEvoCwey", "points": 26.773113 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "YdZfhhjLcQ", "name": "ETyucPiipS", "points": 21.427801 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "jgspkYCoSb", "name": "OLotJJwkwA", "points": 59.92018 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oWBMxpRmYK", "name": "rWTWZezDFC", "points": 9.587106 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "LqPuYBVMHO", "name": "FdwxHFnpmw", "points": 37.545956 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "WLssQjwYdD", "name": "KwcFZbpggS", "points": 60.061428 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "CFbEZZlPBj", "name": "KAcdExeCEp", "points": 83.24783 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hqVUGbTRtY", "name": "YkKPLsHLDk", "points": 53.39632 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "TViJakkgdZ", "name": "xlIlCkgShs", "points": 68.26869 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JQChtxcdBN", "name": "pgicqgvOfL", "points": 29.73249 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "UplQvpwkJS", "name": "UxwuCmfVkf", "points": 81.458755 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "MxXRUUmKYY", "name": "AyjjHlLSlY", "points": 5.1051736 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "QOabLbhmno", "name": "sSMiSCDBjn", "points": 6.178382 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "aGcMnIIWHL", "name": "cTCwxBcajy", "points": 97.90511 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "hZDXIJLriL", "name": "ewSqudjoUF", "points": 93.59539 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GvBPFXHDYJ", "name": "mUOJTZwbkj", "points": 57.023144 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gQFJtyFjJf", "name": "IWCRHcdruo", "points": 48.52044 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ySVlaDNLju", "name": "aMzQjIgmkj", "points": 29.791399 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "nNYeFsivko", "name": "xnSqAGEzHX", "points": 5.294324 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xijFvkMxUh", "name": "aCHrXlvBjS", "points": 22.86222 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "icqZQkiBMv", "name": "DRIIIlglNH", "points": 3.6140492 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NyoICekbEd", "name": "qTJopOgsvp", "points": 49.562626 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ATWjYznKEU", "name": "FoZPJXLJIT", "points": 12.273962 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KSbqpqFHWe", "name": "YAnKswYGaO", "points": 43.206047 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "pesTQGQGhD", "name": "JriisiyxlV", "points": 47.5073 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "swbdUizbeP", "name": "rLnUERVTny", "points": 2.3941646 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "scoqIQRLoB", "name": "DSnhpheAqZ", "points": 22.966156 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "NCnFXxMYCB", "name": "PyqDkMhcst", "points": 98.521416 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "oZEhIBouty", "name": "usQRcpYQDZ", "points": 94.80871 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "IUuVOWYfFd", "name": "GcYILyKPhP", "points": 70.42903 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZzcVLorCqo", "name": "ygdGrRwHJV", "points": 21.726358 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "GEiEhjawYC", "name": "oHCaXsbByO", "points": 80.82534 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "BxiUOZLJmd", "name": "wzGvYraeLE", "points": 14.636214 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "XYpBoXQRTb", "name": "BRRQxcsHRE", "points": 44.358807 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "HtGtZdoFob", "name": "taJRFxmMft", "points": 74.7967 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "eyxjMROvpv", "name": "IsgmqzRBKD", "points": 23.885807 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "FpjrihBiwo", "name": "JqAdBidtiI", "points": 75.02612 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "ZNtFbNTGEP", "name": "UfmszCGrdg", "points": 82.244965 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "xDvAyNaPWV", "name": "ANgbBGDEfU", "points": 23.968576 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "gSlGebdgiY", "name": "amruvFphRL", "points": 29.063879 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "mVibLKmWEC", "name": "YkzrWgUCak", "points": 91.07022 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "obpKPOdknT", "name": "hAlWithecC", "points": 71.46329 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "nHIfYSFsHW", "name": "jbXjqaToQy", "points": 85.49525 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn query_performance_with_float32_actions_4() -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "DxmgxGAnuK", "name": "TxaldfSOMe", "points": 82.31101 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "KWqCrNBgMw", "name": "EqXZVWlneU", "points": 55.501617 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(
                json!({ "email": "JgBVuKXOUC", "name": "ZrQyEJXVgF", "points": 63.576878 }),
            ),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Benchmark {
            base_query: r#"
                    query {
                        User(filter: {points: {_eq: 33}}) {
                            name
                            points
                            email
                        }
                    }
                "#
            .into(),
            optimized_query: r#"
                    query {
                        IndexedUser(filter: {points: {_eq: 33}}) {
                            name
                            points
                            email
                        }
                    }
                "#
            .into(),
            expected_data: json!(m::object(
                json!({ "passed": true, "repetitions": 10, "factor": 2 })
            )),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("go.index.query.performance")]
fn query_performance_with_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: [
            query_performance_with_float32_actions_0(),
            query_performance_with_float32_actions_1(),
            query_performance_with_float32_actions_2(),
            query_performance_with_float32_actions_3(),
            query_performance_with_float32_actions_4(),
        ]
        .concat(),
        ..Default::default()
    }
    .execute();
}
