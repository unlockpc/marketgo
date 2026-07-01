//! 单元测试集合 —— 从 lib.rs 抽出，保持 crate 内子模块以访问 pub(crate) 项。

#[cfg(test)]
mod platform_meta_tests {
    use crate::*;

    #[test]
    fn all_28_keys_have_meta() {
        assert_eq!(PLATFORM_KEYS.len(), 28);
        for k in PLATFORM_KEYS {
            assert!(platform_meta(k).is_some(), "missing meta for {}", k);
        }
    }

    #[test]
    fn mode_counts_14_auto_14_manual() {
        let auto = PLATFORM_KEYS.iter().filter(|k| platform_meta(k).unwrap().mode == "auto").count();
        let manual = PLATFORM_KEYS.iter().filter(|k| platform_meta(k).unwrap().mode == "manual").count();
        assert_eq!(auto, 14, "auto count");
        assert_eq!(manual, 14, "manual count");
    }

    #[test]
    fn spot_checks() {
        let g = platform_meta("github").unwrap();
        assert_eq!((g.scene, g.region, g.mode), ("research", "us", "auto"));
        let x = platform_meta("xiaohongshu").unwrap();
        assert_eq!((x.scene, x.region, x.mode), ("lifestyle", "cn", "manual"));
        // 敌意平台：虽支持 google_oauth 但仍为 manual
        assert_eq!(platform_meta("twitter").unwrap().mode, "manual");
        assert_eq!(platform_meta("reddit").unwrap().mode, "manual");
        // 别名解析
        assert!(platform_meta("redbook").is_some());
        assert!(platform_meta("okjike").is_some());
        // 未知平台
        assert!(platform_meta("unknown_xyz").is_none());
    }

    #[test]
    fn all_six_scenes_present() {
        use std::collections::HashSet;
        let scenes: HashSet<_> = PLATFORM_KEYS.iter().map(|k| platform_meta(k).unwrap().scene).collect();
        for s in ["research", "product", "social", "content", "career", "lifestyle"] {
            assert!(scenes.contains(s), "missing scene {}", s);
        }
    }

    #[test]
    fn catalog_items_cover_all_keys_with_name() {
        // 不依赖 DB：仅验证 name 解析 + meta 覆盖
        for k in PLATFORM_KEYS {
            let m = platform_meta(k).unwrap();
            let name = get_platform_config(k).map(|c| c.name.to_string());
            assert!(name.is_some(), "platform {} 在 get_platform_config 里没有配置", k);
            assert!(!m.scene.is_empty() && !m.region.is_empty() && !m.mode.is_empty());
        }
    }

    #[test]
    fn nurture_homepage_resolves_to_platform_not_google() {
        // 回归：养号导航曾用残缺的 platform_home()，csdn 落到 google.com 兜底 →
        // 在 google 页上检测 csdn 登录必然失败、养号空转。现已统一走 get_platform_home_url()。
        assert_eq!(get_platform_home_url("csdn"), "https://www.csdn.net");
        for p in ["csdn", "zhihu", "weibo", "twitter", "reddit", "linkedin", "github", "v2ex", "devto", "medium"] {
            assert_ne!(
                get_platform_home_url(p),
                "https://www.google.com",
                "{} 养号导航落到 google 兜底，会导致登录检测失败、不真正操作",
                p
            );
        }
    }

    // ===== GitHub 养号：纯逻辑单元测试 =====

    #[test]
    fn gh_domains_keys_unique_and_topics_nonempty() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for d in GH_DOMAINS {
            assert!(seen.insert(d.key), "duplicate domain key {}", d.key);
            assert!(!d.label.is_empty(), "empty label for {}", d.key);
            assert!(!d.topics.is_empty(), "no topics for {}", d.key);
        }
        for k in ["frontend", "backend", "ml", "ai_coding", "devops"] {
            assert!(GH_DOMAINS.iter().any(|d| d.key == k), "missing domain {}", k);
        }
    }


    #[test]
    fn gh_daily_quota_by_phase() {
        assert_eq!(gh_daily_quota("warmup"), (1, 0, 0));
        let (s, f, w) = gh_daily_quota("growth");
        assert!(s >= 1 && s <= 3 && f <= 2 && w <= 1);
        let (s2, _, _) = gh_daily_quota("mature");
        assert!(s2 >= 1);
        assert_eq!(gh_daily_quota("unknown"), (1, 0, 0));
    }

    #[test]
    fn gh_pick_targets_filters_and_limits() {
        use std::collections::HashSet;
        let cands = vec![
            "https://github.com/a/x".to_string(),
            "https://github.com/b/y".to_string(),
            "https://github.com/c/z".to_string(),
        ];
        let mut already = HashSet::new();
        already.insert("https://github.com/a/x".to_string());
        let picked = gh_pick_targets(&cands, &already, 2, 12345);
        assert!(picked.len() <= 2);
        assert!(!picked.contains(&"https://github.com/a/x".to_string()));
        for p in &picked { assert!(cands.contains(p)); }
    }

    #[test]
    fn gh_pick_targets_empty_when_all_acted() {
        use std::collections::HashSet;
        let cands = vec!["https://github.com/a/x".to_string()];
        let already: HashSet<String> = cands.iter().cloned().collect();
        assert!(gh_pick_targets(&cands, &already, 3, 1).is_empty());
    }

    // ===== X 养号：纯逻辑单元测试 =====

    #[test]
    fn x_niches_keys_unique_and_16_dirs() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for n in X_NICHES {
            assert!(seen.insert(n.key), "duplicate niche key {}", n.key);
            assert!(!n.label.is_empty() && !n.keywords.is_empty(), "bad niche {}", n.key);
        }
        assert_eq!(X_NICHES.len(), 16, "X 官方顶层方向应为 16 个");
        for k in ["technology", "business_finance", "science", "gaming", "music"] {
            assert!(X_NICHES.iter().any(|n| n.key == k), "missing niche {}", k);
        }
    }


    #[test]
    fn x_daily_quota_by_phase() {
        assert_eq!(x_daily_quota("warmup"), (2, 0, 0));
        let (l, f, e) = x_daily_quota("growth");
        assert!(l >= 3 && f >= 1 && e >= 1);
        let (l2, _, _) = x_daily_quota("mature");
        assert!(l2 >= 1);
        assert_eq!(x_daily_quota("unknown"), (1, 0, 0));
    }

    #[test]
    fn x_jitter_quota_stays_in_bounds() {
        // 多个 seed 下，抖动后的配额都应落在预期区间，且转推含 0
        let mut saw_zero_engage = false;
        let mut saw_max_engage = false;
        for s in 0u64..2000 {
            // mature 基准 (5,1,3)
            let (l, f, e) = x_jitter_quota((5, 1, 3), s.wrapping_mul(2654435761));
            assert!((3..=5).contains(&l), "like {} 越界", l);
            assert!((0..=1).contains(&f), "follow {} 越界", f);
            assert!((0..=3).contains(&e), "engage {} 越界", e);
            if e == 0 { saw_zero_engage = true; }
            if e == 3 { saw_max_engage = true; }
        }
        assert!(saw_zero_engage, "转推从未取到 0（应允许某轮不转推）");
        assert!(saw_max_engage, "转推从未取到上限 3");
        // base 为 0 的维度恒为 0
        assert_eq!(x_jitter_quota((2, 0, 0), 12345), (x_jitter_quota((2, 0, 0), 12345).0, 0, 0));
        let (wl, _, _) = x_jitter_quota((2, 0, 0), 999);
        assert!((1..=2).contains(&wl));
    }

    #[test]
    fn weibo_like_range_by_phase() {
        assert_eq!(weibo_like_range("warmup"), (1, 2));
        assert_eq!(weibo_like_range("growth"), (1, 3));
        assert_eq!(weibo_like_range("mature"), (1, 3));
        assert_eq!(weibo_like_range("unknown"), (1, 1));
    }

    #[test]
    fn weibo_pick_likes_stays_in_bounds() {
        // 成长/成熟 (1,3)：随机取值应落在 [1,3]，且能取到下界 1 与上界 3
        let (mut saw_lo, mut saw_hi) = (false, false);
        for s in 0u64..3000 {
            let l = weibo_pick_likes((1, 3), s.wrapping_mul(2654435761));
            assert!((1..=3).contains(&l), "like {} 越界", l);
            if l == 1 { saw_lo = true; }
            if l == 3 { saw_hi = true; }
        }
        assert!(saw_lo && saw_hi, "随机未覆盖区间端点 lo={} hi={}", saw_lo, saw_hi);
        // 预热 (1,2)：恒落在 [1,2]
        for s in 0u64..1000 {
            let l = weibo_pick_likes((1, 2), s.wrapping_mul(40503));
            assert!((1..=2).contains(&l), "warmup like {} 越界", l);
        }
        // 兜底 (1,1) 恒为 1；非法区间 → 0
        assert_eq!(weibo_pick_likes((1, 1), 123), 1);
        assert_eq!(weibo_pick_likes((0, 0), 123), 0);
        assert_eq!(weibo_pick_likes((3, 1), 123), 0);
    }

    #[test]
    fn weibo_reply_quota_by_phase() {
        assert_eq!(weibo_reply_quota("growth"), 1);
        assert_eq!(weibo_reply_quota("mature"), 1);
        assert_eq!(weibo_reply_quota("warmup"), 0); // 预热不评论/转帖
        assert_eq!(weibo_reply_quota("unknown"), 0);
    }

    #[test]
    fn weibo_classify_health_maps() {
        assert_eq!(weibo_classify_health("请完成验证码后继续"), Some("captcha"));
        assert_eq!(weibo_classify_health("需要安全验证"), Some("captcha"));
        assert_eq!(weibo_classify_health("您的账号异常，请联系客服"), Some("banned"));
        assert_eq!(weibo_classify_health("操作过于频繁，请稍后再试"), Some("restricted"));
        assert_eq!(weibo_classify_health("已超过当日点赞上限"), Some("restricted"));
        assert_eq!(weibo_classify_health("AI 工具 的搜索结果"), None);
    }

    #[test]
    fn nurture_phase_two_durations() {
        // 预热10 + 成长10：成熟从第 20 天起（= 旧的 2×warmup 行为）
        assert_eq!(nurture_phase_and_target(0, 10, 10, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(9, 10, 10, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(10, 10, 10, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(19, 10, 10, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(20, 10, 10, 2, 4).0, "mature");
        // 两段独立：预热7 + 成长30 → 成熟从第 37 天起
        assert_eq!(nurture_phase_and_target(6, 7, 30, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(7, 7, 30, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(36, 7, 30, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(37, 7, 30, 2, 4).0, "mature");
        // 成长时长为 0：预热完直接进成熟
        assert_eq!(nurture_phase_and_target(5, 5, 0, 2, 4).0, "mature");
    }

    #[test]
    fn x_l3_gate() {
        // 闸门跟随 warmup：号龄未到预热期末或无 L1 历史都不解锁
        assert!(!x_l3_allowed(6, 10, 50));    // 号龄 < warmup
        assert!(!x_l3_allowed(20, 10, 0));    // 无 L1 历史
        assert!(x_l3_allowed(10, 10, 1));     // 恰好走完预热期 + 有 L1
        assert!(x_l3_allowed(40, 10, 100));
        // 周期调长后门槛同步后移
        assert!(!x_l3_allowed(14, 21, 1));
        assert!(x_l3_allowed(21, 21, 1));
    }

    #[test]
    fn x_classify_health_maps() {
        assert_eq!(x_classify_health("Your account is suspended"), Some("banned"));
        assert_eq!(x_classify_health("Your account has been locked"), Some("locked"));
        assert_eq!(x_classify_health("You are unable to perform this action"), Some("restricted"));
        assert_eq!(x_classify_health("Rate limit exceeded, try again later"), Some("restricted"));
        // 中文界面（简/繁）
        assert_eq!(x_classify_health("你的账号已被冻结"), Some("banned"));
        assert_eq!(x_classify_health("验证你的身份以继续"), Some("locked"));
        assert_eq!(x_classify_health("无法执行此操作，请稍后再试"), Some("restricted"));
        // 顺带测粉丝数解析
        assert_eq!(x_parse_count("1,234 Followers"), Some(1234));
        assert_eq!(x_parse_count("1.2K Followers"), Some(1200));
        assert_eq!(x_parse_count("3.4M"), Some(3_400_000));
        assert_eq!(x_parse_count("Followers 5,678"), Some(5678));
        assert_eq!(x_parse_count("no digits"), None);
        // 正常页面（含登录后导航词）不误报
        assert_eq!(x_classify_health("Home timeline, Post, Notifications, Messages"), None);
        assert_eq!(x_classify_health("首页 发推 通知 私信"), None);
    }
}

#[cfg(test)]
mod gh_db_tests {
    use crate::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, gh_domains TEXT);
            CREATE TABLE gh_actions_log (id TEXT PRIMARY KEY, account_id TEXT, action_type TEXT, target TEXT, date TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
        ").unwrap();
        c
    }

    #[test]
    fn record_and_already_acted() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        assert!(!gh_already_acted(&c, "acc1", tgt));
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        assert!(gh_already_acted(&c, "acc1", tgt));
        assert!(!gh_already_acted(&c, "acc2", tgt));
    }

    #[test]
    fn target_persona_count_cross_account() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap();
        assert_eq!(gh_target_persona_count(&c, tgt), 2);
    }
}

#[cfg(test)]
mod x_db_tests {
    use crate::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, x_niches TEXT);
            CREATE TABLE x_actions_log (id TEXT PRIMARY KEY, account_id TEXT, action_type TEXT, target TEXT, date TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
        ").unwrap();
        c
    }

    #[test]
    fn record_and_already_acted() {
        let c = setup();
        let tgt = "https://x.com/u/status/1";
        assert!(!x_already_acted(&c, "acc1", tgt));
        x_record_action(&c, "acc1", "like", tgt).unwrap();
        assert!(x_already_acted(&c, "acc1", tgt));
        assert!(!x_already_acted(&c, "acc2", tgt));
    }

    #[test]
    fn target_persona_count_cross_account() {
        let c = setup();
        let tgt = "https://x.com/u/status/1";
        x_record_action(&c, "acc1", "like", tgt).unwrap();
        x_record_action(&c, "acc2", "like", tgt).unwrap();
        x_record_action(&c, "acc2", "like", tgt).unwrap();
        assert_eq!(x_target_persona_count(&c, tgt), 2);
    }
}

#[cfg(test)]
mod topics_tests {
    use crate::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, platform TEXT, nurture_topics TEXT);
            CREATE TABLE custom_topics (key TEXT PRIMARY KEY, platform TEXT NOT NULL, label TEXT NOT NULL, keywords TEXT);
        ").unwrap();
        c
    }

    #[test]
    fn builtin_maps_each_platform() {
        assert!(builtin_topics("github").iter().any(|t| t.key == "frontend" && !t.keywords.is_empty()));
        assert!(builtin_topics("twitter").iter().any(|t| t.key == "technology"));
        assert!(builtin_topics("segmentfault").iter().any(|t| t.key == "frontend"));
        let beauty = builtin_topics("xiaohongshu").into_iter().find(|t| t.key == "beauty").unwrap();
        assert_eq!(beauty.label, "美妆护肤");
        assert_eq!(beauty.keywords, vec!["美妆护肤".to_string()]);
        // 微博内置主题：每个展开成一组具体词（不是 label 本身）
        let sports = builtin_topics("weibo").into_iter().find(|t| t.key == "sports").unwrap();
        assert_eq!(sports.label, "体育运动");
        assert!(sports.keywords.contains(&"足球".to_string()) && sports.keywords.len() >= 3);
        assert!(builtin_topics("unknown").is_empty());
    }

    #[test]
    fn weibo_collect_follow_quota_and_quality() {
        use crate::{weibo_collect_quota, weibo_follow_quota, weibo_author_passes_quality, weibo_uid_from_url, weibo_parse_fans};
        // 收藏：各期上界恒 1；关注：预热 0，成长/成熟 2
        for p in ["warmup", "growth", "mature"] { assert_eq!(weibo_collect_quota(p), 1); }
        assert_eq!(weibo_follow_quota("warmup"), 0);
        assert_eq!(weibo_follow_quota("growth"), 2);
        assert_eq!(weibo_follow_quota("mature"), 2);
        // 粉丝解析：万/亿/纯数
        assert_eq!(weibo_parse_fans("数码闲聊站 348.3万粉丝 134关注"), 3_483_000);
        assert_eq!(weibo_parse_fans("某大V 1.2亿粉丝"), 120_000_000);
        assert_eq!(weibo_parse_fans("小号 832粉丝 50关注"), 832);
        assert_eq!(weibo_parse_fans("没有粉丝字样"), 0);
        // 质量门：≥1000 才过
        assert!(weibo_author_passes_quality(1000));
        assert!(!weibo_author_passes_quality(999));
        // uid 提取：weibo.com/<uid> 与 weibo.com/u/<uid>
        assert_eq!(weibo_uid_from_url("//weibo.com/6048569942?refer_flag=1").as_deref(), Some("6048569942"));
        assert_eq!(weibo_uid_from_url("https://weibo.com/u/123456").as_deref(), Some("123456"));
        assert_eq!(weibo_uid_from_url("https://weibo.com/n/某昵称"), None); // 非数字 uid
    }

    #[test]
    fn weibo_topic_fallback_builtin_vs_custom() {
        // 内置主题 → 写死的具体关键词（无 AI key 时的回退）
        let sports = builtin_topic_fallback("weibo", "sports", "体育运动");
        assert!(sports.contains(&"足球".to_string()));
        // 自定义/未知 key → 回退用 label 本身
        assert_eq!(builtin_topic_fallback("weibo", "u-custom", "露营装备"), vec!["露营装备".to_string()]);
    }

    #[test]
    fn catalog_platform_isolated_builtin_first() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u2','github','我的库',NULL)", []).unwrap();
        let xhs = topics_catalog_from(&c, "xiaohongshu");
        assert!(xhs[0].builtin);
        assert!(xhs.iter().any(|i| i.key == "u1" && !i.builtin));
        assert!(!xhs.iter().any(|i| i.key == "u2")); // 跨平台隔离
    }

    #[test]
    fn read_account_topics_roundtrip() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"food\"]')", []).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string(), "food".to_string()]);
        assert!(account_topics(&c, "nope").is_empty());
    }

    #[test]
    fn set_filters_unknown_and_roundtrips() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform) VALUES ('a1','xiaohongshu')", []).unwrap();
        set_account_topics_conn(&c, "a1", &["beauty".to_string(), "ghost".to_string()]).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string()]);
    }

    #[test]
    fn set_keeps_custom_of_same_platform() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform) VALUES ('a1','xiaohongshu')", []).unwrap();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        set_account_topics_conn(&c, "a1", &["u1".to_string()]).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["u1".to_string()]);
    }

    #[test]
    fn add_trims_and_rejects_dup_per_platform() {
        let c = setup();
        let it = add_custom_topic_conn(&c, "xiaohongshu", "  露营装备  ").unwrap();
        assert_eq!(it.label, "露营装备");
        assert!(!it.builtin);
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "   ").is_err());      // 空
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "美妆护肤").is_err()); // 内置重名
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "露营装备").is_err()); // 自定义重名
        add_custom_topic_conn(&c, "github", "露营装备").unwrap();              // 不同平台同名 OK
    }

    #[test]
    fn delete_custom_strips_and_rejects_builtin() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"u1\"]')", []).unwrap();
        delete_custom_topic_conn(&c, "u1").unwrap();
        assert!(!topics_catalog_from(&c, "xiaohongshu").iter().any(|i| i.key == "u1"));
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string()]);
        assert!(delete_custom_topic_conn(&c, "beauty").is_err()); // 内置不可删
    }

    #[test]
    fn topic_keywords_builtin_and_custom_fallback() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"u1\"]')", []).unwrap();
        let kws = account_topic_keywords(&c, "a1");
        assert!(kws.contains(&"美妆护肤".to_string())); // 内置 xhs：keywords=label
        assert!(kws.contains(&"露营装备".to_string())); // 自定义无 keywords → 回退 label
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('g1','github','[\"frontend\"]')", []).unwrap();
        assert!(account_topic_keywords(&c, "g1").contains(&"react".to_string())); // github：keywords 来自 topics
    }
}

#[cfg(test)]
mod xhs_runner_tests {
    use crate::*;
    use rusqlite::Connection;

    fn setup_strategies() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE nurture_strategies (platform TEXT PRIMARY KEY, warmup_days INTEGER, growth_days INTEGER, daily_sessions_min INTEGER, daily_sessions_max INTEGER);
        ").unwrap();
        c
    }

    #[test]
    fn xhs_default_sets_7_5() {
        let c = setup_strategies();
        // 不论原值（含 v4 播种的 30）→ 统一改 7/5
        c.execute("INSERT INTO nurture_strategies (platform,warmup_days,growth_days,daily_sessions_min,daily_sessions_max) VALUES ('xiaohongshu',30,NULL,3,6)", []).unwrap();
        assert_eq!(apply_xhs_strategy_default(&c), 1);
        let (w, g): (i64, i64) = c.query_row("SELECT warmup_days, growth_days FROM nurture_strategies WHERE platform='xiaohongshu'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((w, g), (7, 5));
    }

    #[test]
    fn xhs_default_only_touches_xiaohongshu() {
        let c = setup_strategies();
        c.execute("INSERT INTO nurture_strategies (platform,warmup_days,growth_days,daily_sessions_min,daily_sessions_max) VALUES ('twitter',14,NULL,2,4)", []).unwrap();
        apply_xhs_strategy_default(&c);
        let w: i64 = c.query_row("SELECT warmup_days FROM nurture_strategies WHERE platform='twitter'", [], |r| r.get(0)).unwrap();
        assert_eq!(w, 14); // 不动其它平台
    }

    #[test]
    fn phase_intensity_by_stage() {
        // 返回 (搜索次数, 是否点赞)；阅读篇数与点赞次数在 runner 里随机。各阶段均点赞。
        assert_eq!(crate::nurture::xhs_phase_intensity("warmup"), (2, true)); // 预热也点赞
        assert_eq!(crate::nurture::xhs_phase_intensity("growth"), (3, true)); // 成长搜索更多
        assert_eq!(crate::nurture::xhs_phase_intensity("mature"), (2, true)); // 成熟维持
        assert_eq!(crate::nurture::xhs_phase_intensity("other"), (2, true));  // 兜底=预热
    }

    #[test]
    fn expand_query_always_appends_suffix() {
        use crate::nurture::xhs_expand_query;
        use std::collections::HashSet;
        // 总是「原主题 + 空格 + 非空后缀」，绝不返回光秃秃的原词
        let mut suffixes = HashSet::new();
        for seed in 0u64..200 {
            let q = xhs_expand_query("数码科技", seed);
            assert!(q.starts_with("数码科技 "), "应始终拼接后缀: {}", q);
            let suffix = q.strip_prefix("数码科技 ").unwrap();
            assert!(!suffix.is_empty(), "后缀不应为空: {}", q);
            suffixes.insert(suffix.to_string());
        }
        // 后缀应有多样性（不是固定一个）
        assert!(suffixes.len() >= 5, "后缀应多样，实际只有 {} 种", suffixes.len());
    }

    #[test]
    fn x_reply_quota_by_phase() {
        use crate::nurture::x_reply_quota;
        assert_eq!(x_reply_quota("warmup"), 1);
        assert_eq!(x_reply_quota("growth"), 1);
        assert_eq!(x_reply_quota("mature"), 2);
        assert_eq!(x_reply_quota("other"), 1);
    }

    #[test]
    fn xhs_reply_quota_by_phase() {
        use crate::nurture::xhs_reply_quota;
        // 小红书评论风控敏感：预热期不评论，成长/成熟期每轮顶多 1 条
        assert_eq!(xhs_reply_quota("warmup"), 0);
        assert_eq!(xhs_reply_quota("growth"), 1);
        assert_eq!(xhs_reply_quota("mature"), 1);
        assert_eq!(xhs_reply_quota("other"), 0);
    }

    #[test]
    fn xhs_filler_comment_detection() {
        use crate::nurture::xhs_is_filler_comment;
        // 灌水：套话、纯@、太短、纯表情/标点
        assert!(xhs_is_filler_comment("学到了，感谢分享"));   // 套话(就是测试发的那条)
        assert!(xhs_is_filler_comment("支持"));
        assert!(xhs_is_filler_comment("码住"));
        assert!(xhs_is_filler_comment("沙发"));
        assert!(xhs_is_filler_comment("@小红薯692559BB"));     // 纯@提及
        assert!(xhs_is_filler_comment("好"));                  // 太短
        assert!(xhs_is_filler_comment("！！！！！"));            // 纯标点
        assert!(xhs_is_filler_comment("666"));
        // 非灌水：有观点/问题/吐槽，值得回复
        assert!(!xhs_is_filler_comment("虽然但是做 skill 不就是这样的么？看不出来这算什么更新"));
        assert!(!xhs_is_filler_comment("这个很消耗token吧"));
        assert!(!xhs_is_filler_comment("别更新了 更新一次崩一次"));
        assert!(!xhs_is_filler_comment("后面 帮我点餐 就很容易了"));
    }

    #[test]
    fn xhs_collect_and_follow_quota_by_phase() {
        use crate::nurture::{xhs_collect_quota, xhs_follow_quota};
        // 收藏：各期上界恒 1（含预热期，收藏几乎无风控）
        for p in ["warmup", "growth", "mature", "other"] {
            assert_eq!(xhs_collect_quota(p), 1, "收藏配额 {}", p);
        }
        // 关注：预热 0（新号不关注），成长/成熟上界 2
        assert_eq!(xhs_follow_quota("warmup"), 0);
        assert_eq!(xhs_follow_quota("growth"), 2);
        assert_eq!(xhs_follow_quota("mature"), 2);
        assert_eq!(xhs_follow_quota("other"), 0);
    }

    #[test]
    fn xhs_parse_count_handles_wan_and_plain() {
        use crate::nurture::xhs_parse_count;
        assert_eq!(xhs_parse_count("570"), 570);
        assert_eq!(xhs_parse_count(" 4275 "), 4275);
        assert_eq!(xhs_parse_count("1.2万"), 12000);
        assert_eq!(xhs_parse_count("3.5w"), 35000);
        assert_eq!(xhs_parse_count("2W"), 20000);
        assert_eq!(xhs_parse_count("abc"), 0); // 解析失败兜底 0
        assert_eq!(xhs_parse_count(""), 0);
    }

    #[test]
    fn xhs_parse_profile_stats_extracts_fans_and_likes() {
        use crate::nurture::xhs_parse_profile_stats;
        assert_eq!(xhs_parse_profile_stats("90 关注 570 粉丝 4275 获赞与收藏"), (570, 4275));
        assert_eq!(xhs_parse_profile_stats("1200 关注 1.2万 粉丝 50万 获赞与收藏"), (12000, 500000));
        // 缺字段 → 该项 0
        assert_eq!(xhs_parse_profile_stats("乱七八糟没有数字"), (0, 0));
    }

    #[test]
    fn xhs_author_passes_quality_gate() {
        use crate::nurture::xhs_author_passes_quality;
        assert!(xhs_author_passes_quality(500, 0));      // 粉丝达标
        assert!(xhs_author_passes_quality(0, 3000));     // 获赞达标
        assert!(xhs_author_passes_quality(600, 100));    // 任一达标即可
        assert!(!xhs_author_passes_quality(499, 2999));  // 都不达标 → 不关注
        assert!(!xhs_author_passes_quality(0, 0));
    }

    #[test]
    fn xhs_author_id_from_url_extracts() {
        use crate::nurture::xhs_author_id_from_url;
        assert_eq!(
            xhs_author_id_from_url("https://www.xiaohongshu.com/user/profile/61ab89e7000000001000cbd7?xsec_token=ABC=&x=1").as_deref(),
            Some("61ab89e7000000001000cbd7")
        );
        assert_eq!(xhs_author_id_from_url("/user/profile/abc123").as_deref(), Some("abc123"));
        assert_eq!(xhs_author_id_from_url("https://www.xiaohongshu.com/explore/xxx"), None); // 非作者链接
    }

    #[test]
    fn reply_style_tone_maps_known_and_defaults() {
        use crate::ai::reply_style_tone;
        // 已知风格各有不同语气；未知/空 → 默认真诚(sincere)
        assert!(reply_style_tone("professional").contains("专业"));
        assert!(reply_style_tone("humorous").contains("幽默"));
        assert!(reply_style_tone("casual").contains("随性"));
        assert!(reply_style_tone("enthusiastic").contains("热情"));
        let def = reply_style_tone("sincere");
        assert!(def.contains("真诚"));
        assert_eq!(reply_style_tone("不存在的风格"), def);
        assert_eq!(reply_style_tone(""), def);
    }

    #[test]
    fn x_clean_tweet_text_length_gate() {
        use crate::nurture::x_clean_tweet_text;
        // 太短 / 空 → None
        assert_eq!(x_clean_tweet_text(""), None);
        assert_eq!(x_clean_tweet_text("  short  "), None);      // trim 后 5 字符
        // 足够长 → Some(trimmed)
        let long = "  this is a long enough tweet body  ";
        assert_eq!(x_clean_tweet_text(long), Some("this is a long enough tweet body".to_string()));
    }

    #[test]
    fn sf_expand_always_appends_chinese_suffix() {
        use crate::nurture::sf_expand_query;
        for seed in 0u64..100 {
            let q = sf_expand_query("Rust", seed);
            assert!(q.starts_with("Rust "), "思否应始终拼后缀: {}", q);
            assert!(q.len() > "Rust ".len(), "后缀不应为空: {}", q);
        }
    }

    #[test]
    fn x_expand_appends_for_words_but_keeps_hashtags() {
        use crate::nurture::x_expand_query;
        // 普通词：总是拼英文后缀
        for seed in 0u64..50 {
            let q = x_expand_query("machine learning", seed);
            assert!(q.starts_with("machine learning "), "普通词应拼后缀: {}", q);
        }
        // hashtag：保持原样，不拼
        for seed in 0u64..50 {
            assert_eq!(x_expand_query("#AI", seed), "#AI", "hashtag 不应拼后缀");
        }
    }
}

#[cfg(test)]
mod login_precheck_tests {
    use crate::nurture::nurture_requires_login;

    #[test]
    fn requires_login_only_for_dedicated_runners() {
        // 有专属 runner、未登录会直接报错的平台 → 预检
        for p in ["github", "twitter", "x", "segmentfault", "xiaohongshu", "redbook", "weibo"] {
            assert!(nurture_requires_login(p), "{} 应纳入登录预检", p);
        }
        // 大小写不敏感
        assert!(nurture_requires_login("GitHub"));
        assert!(nurture_requires_login("XiaoHongShu"));
        assert!(nurture_requires_login("Weibo"));
        // 通用滚动平台不强依赖登录 → 不预检
        for p in ["zhihu", "reddit", "medium", "v2ex", "", "unknown"] {
            assert!(!nurture_requires_login(p), "{} 不应纳入登录预检", p);
        }
    }
}

#[cfg(test)]
mod proxy_normalize_tests {
    use crate::normalize_proxy;

    #[test]
    fn normalize_proxy_rules() {
        // 空 / 纯空白 → None
        assert_eq!(normalize_proxy("").unwrap(), None);
        assert_eq!(normalize_proxy("   ").unwrap(), None);
        // host:port 无协议 → 补 socks5://
        assert_eq!(normalize_proxy("1.2.3.4:18080").unwrap(), Some("socks5://1.2.3.4:18080".to_string()));
        // 带账号密码
        assert_eq!(normalize_proxy("u:p@host:1080").unwrap(), Some("socks5://u:p@host:1080".to_string()));
        // 已带协议保持原样
        assert_eq!(normalize_proxy("http://h:8080").unwrap(), Some("http://h:8080".to_string()));
        assert_eq!(normalize_proxy("socks5://h:1080").unwrap(), Some("socks5://h:1080".to_string()));
        // 非法协议 → Err
        assert!(normalize_proxy("ftp://h:21").is_err());
    }
}

#[cfg(test)]
mod drop_fixed_personas_tests {
    use rusqlite::{params, Connection};

    /// 模拟 drop_fixed_personas_once 的纯 DB 部分：fixed persona 删除 + 账号解除关联。
    fn drop_fixed_sql(conn: &Connection) {
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM personas WHERE ip_mode='fixed'").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().filter_map(|x| x.ok()).collect()
        };
        for id in &ids {
            conn.execute("UPDATE accounts SET persona_id=NULL WHERE persona_id=?1", params![id]).unwrap();
            conn.execute("DELETE FROM personas WHERE id=?1", params![id]).unwrap();
        }
    }

    #[test]
    fn fixed_personas_dropped_accounts_unlinked_but_kept() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE personas (id TEXT PRIMARY KEY, ip_mode TEXT);
            CREATE TABLE accounts (id TEXT PRIMARY KEY, persona_id TEXT);
            INSERT INTO personas (id, ip_mode) VALUES ('gm1','airport'), ('fx1','fixed'), ('fx2','fixed');
            INSERT INTO accounts (id, persona_id) VALUES ('a1','gm1'), ('a2','fx1'), ('a3','fx2');
        ").unwrap();
        drop_fixed_sql(&c);
        // fixed 身份被删，airport 保留
        let persona_n: i64 = c.query_row("SELECT COUNT(*) FROM personas", [], |r| r.get(0)).unwrap();
        assert_eq!(persona_n, 1);
        // 账号全部保留
        let acct_n: i64 = c.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0)).unwrap();
        assert_eq!(acct_n, 3);
        // 原挂 fixed 的账号变未归属
        let unlinked: i64 = c.query_row("SELECT COUNT(*) FROM accounts WHERE persona_id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(unlinked, 2);
        // 挂 airport 的账号关联不变
        let a1: Option<String> = c.query_row("SELECT persona_id FROM accounts WHERE id='a1'", [], |r| r.get(0)).unwrap();
        assert_eq!(a1, Some("gm1".to_string()));
    }
}
