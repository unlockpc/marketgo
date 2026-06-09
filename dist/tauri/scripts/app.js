"use strict";
(() => {
  // node_modules/@tauri-apps/api/external/tslib/tslib.es6.js
  function __classPrivateFieldGet(receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
  }
  function __classPrivateFieldSet(receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value), value;
  }

  // node_modules/@tauri-apps/api/core.js
  var _Channel_onmessage;
  var _Channel_nextMessageIndex;
  var _Channel_pendingMessages;
  var _Channel_messageEndIndex;
  var _Resource_rid;
  var SERIALIZE_TO_IPC_FN = "__TAURI_TO_IPC_KEY__";
  function transformCallback(callback, once = false) {
    return window.__TAURI_INTERNALS__.transformCallback(callback, once);
  }
  var Channel = class {
    constructor(onmessage) {
      _Channel_onmessage.set(this, void 0);
      _Channel_nextMessageIndex.set(this, 0);
      _Channel_pendingMessages.set(this, []);
      _Channel_messageEndIndex.set(this, void 0);
      __classPrivateFieldSet(this, _Channel_onmessage, onmessage || (() => {
      }), "f");
      this.id = transformCallback((rawMessage) => {
        const index = rawMessage.index;
        if ("end" in rawMessage) {
          if (index == __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")) {
            this.cleanupCallback();
          } else {
            __classPrivateFieldSet(this, _Channel_messageEndIndex, index, "f");
          }
          return;
        }
        const message = rawMessage.message;
        if (index == __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")) {
          __classPrivateFieldGet(this, _Channel_onmessage, "f").call(this, message);
          __classPrivateFieldSet(this, _Channel_nextMessageIndex, __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") + 1, "f");
          while (__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") in __classPrivateFieldGet(this, _Channel_pendingMessages, "f")) {
            const message2 = __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")];
            __classPrivateFieldGet(this, _Channel_onmessage, "f").call(this, message2);
            delete __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")];
            __classPrivateFieldSet(this, _Channel_nextMessageIndex, __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") + 1, "f");
          }
          if (__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") === __classPrivateFieldGet(this, _Channel_messageEndIndex, "f")) {
            this.cleanupCallback();
          }
        } else {
          __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[index] = message;
        }
      });
    }
    cleanupCallback() {
      window.__TAURI_INTERNALS__.unregisterCallback(this.id);
    }
    set onmessage(handler) {
      __classPrivateFieldSet(this, _Channel_onmessage, handler, "f");
    }
    get onmessage() {
      return __classPrivateFieldGet(this, _Channel_onmessage, "f");
    }
    [(_Channel_onmessage = /* @__PURE__ */ new WeakMap(), _Channel_nextMessageIndex = /* @__PURE__ */ new WeakMap(), _Channel_pendingMessages = /* @__PURE__ */ new WeakMap(), _Channel_messageEndIndex = /* @__PURE__ */ new WeakMap(), SERIALIZE_TO_IPC_FN)]() {
      return `__CHANNEL__:${this.id}`;
    }
    toJSON() {
      return this[SERIALIZE_TO_IPC_FN]();
    }
  };
  async function invoke(cmd, args = {}, options) {
    return window.__TAURI_INTERNALS__.invoke(cmd, args, options);
  }
  _Resource_rid = /* @__PURE__ */ new WeakMap();

  // src/tauri-frontend/app.ts
  var currentLanguage = "zh";
  var translations = {
    zh: {
      // Navigation
      "nav.dashboard": "\u4EEA\u8868\u76D8",
      "nav.campaigns": "\u63A8\u5E7F\u6D3B\u52A8",
      "nav.products": "\u4EA7\u54C1\u7BA1\u7406",
      "nav.publish": "\u53D1\u5E03\u5185\u5BB9",
      "nav.articles": "\u6587\u7AE0\u7BA1\u7406",
      "nav.engage": "\u4E92\u52A8\u8425\u9500",
      "nav.accounts": "\u8D26\u53F7\u7BA1\u7406",
      "nav.tasks": "\u4EFB\u52A1\u961F\u5217",
      "nav.statistics": "\u7EDF\u8BA1\u5206\u6790",
      "nav.settings": "\u8BBE\u7F6E",
      // Dashboard
      "dashboard.title": "\u4EEA\u8868\u76D8",
      "dashboard.activeTasks": "\u6D3B\u52A8\u4EFB\u52A1",
      "dashboard.publishedToday": "\u4ECA\u65E5\u53D1\u5E03",
      "dashboard.accountHealth": "\u8D26\u53F7\u5065\u5EB7",
      "dashboard.successRate": "\u6210\u529F\u7387",
      "dashboard.activeCampaigns": "\u6D3B\u52A8\u4E2D\u7684\u63A8\u5E7F",
      "dashboard.newCampaign": "+ \u65B0\u5EFA\u63A8\u5E7F",
      "dashboard.noCampaigns": "\u6682\u65E0\u6D3B\u52A8\u63A8\u5E7F\uFF0C\u521B\u5EFA\u4E00\u4E2A\u5F00\u59CB\u53D1\u5E03\uFF01",
      "dashboard.recentActivity": "\u6700\u8FD1\u6D3B\u52A8",
      "dashboard.noActivity": "\u6682\u65E0\u6700\u8FD1\u6D3B\u52A8",
      "dashboard.platformHealth": "\u5E73\u53F0\u5065\u5EB7",
      "dashboard.quickActions": "\u5FEB\u6377\u64CD\u4F5C",
      "dashboard.addProduct": "\u6DFB\u52A0\u4EA7\u54C1",
      "dashboard.addAccount": "\u6DFB\u52A0\u8D26\u53F7",
      "dashboard.createContent": "\u521B\u5EFA\u5185\u5BB9",
      "dashboard.startEngage": "\u5F00\u59CB\u4E92\u52A8",
      // Accounts
      "accounts.title": "\u5E73\u53F0\u8D26\u53F7",
      "accounts.addAccount": "+ \u6DFB\u52A0\u8D26\u53F7",
      "accounts.overallHealth": "\u6574\u4F53\u5065\u5EB7",
      "accounts.active": "\u6D3B\u8DC3",
      "accounts.warning": "\u8B66\u544A",
      "accounts.banned": "\u5C01\u7981",
      "accounts.warming": "\u517B\u53F7\u4E2D",
      "accounts.autoRegister": "\u81EA\u52A8\u6CE8\u518C (\u901A\u8FC7 Unzoo)",
      "accounts.checking": "\u68C0\u67E5\u4E2D...",
      "accounts.connectGmail": "\u8FDE\u63A5 Gmail",
      "accounts.selectPlatforms": "\u9009\u62E9\u5E73\u53F0",
      "accounts.phone": "\u624B\u673A\u53F7",
      "accounts.googleOAuth": "Google OAuth",
      "accounts.selectAll": "\u5168\u9009",
      "accounts.deselectAll": "\u53D6\u6D88\u5168\u9009",
      "accounts.autoLogin": "\u81EA\u52A8\u767B\u5F55/\u6CE8\u518C\u6240\u9009",
      "accounts.syncAll": "\u540C\u6B65\u5168\u90E8",
      "accounts.syncAllHint": '"\u540C\u6B65\u5168\u90E8" \u5C06\u68C0\u67E5\u4F60\u5728 Unzoo \u6D4F\u89C8\u5668\u4E2D\u5DF2\u767B\u5F55\u7684\u5E73\u53F0',
      "accounts.existingAccounts": "\u73B0\u6709\u8D26\u53F7",
      "accounts.delete": "\u5220\u9664",
      "accounts.useGlobalProfile": "-- \u4F7F\u7528\u5168\u5C40 Profile --",
      "accounts.createProfile": "\u521B\u5EFA Profile",
      "accounts.noAccounts": "\u6682\u65E0\u8D26\u53F7",
      "accounts.autoRegisterHint": "\u4F7F\u7528\u81EA\u52A8\u6CE8\u518C\u6216\u624B\u52A8\u6DFB\u52A0\u8D26\u53F7",
      // Nurturing (养号)
      "nurture.title": "\u8D26\u53F7\u517B\u62A4",
      "nurture.description": "\u6A21\u62DF\u6B63\u5E38\u7528\u6237\u6D4F\u89C8\u884C\u4E3A\uFF0C\u63D0\u5347\u8D26\u53F7\u6743\u91CD",
      "nurture.quickNurture": "\u5FEB\u901F\u517B\u53F7",
      "nurture.startNurture": "\u5F00\u59CB\u517B\u53F7",
      "nurture.stopNurture": "\u505C\u6B62\u517B\u53F7",
      "nurture.totalTime": "\u7D2F\u8BA1\u517B\u53F7\u65F6\u95F4",
      "nurture.lastNurture": "\u4E0A\u6B21\u517B\u53F7",
      "nurture.hours": "\u5C0F\u65F6",
      "nurture.days": "\u5929",
      "nurture.seconds": "\u79D2",
      "nurture.selectAccount": "\u9009\u62E9\u8981\u517B\u62A4\u7684\u8D26\u53F7",
      "nurture.duration": "\u6D4F\u89C8\u65F6\u957F",
      "nurture.30s": "30 \u79D2",
      "nurture.60s": "1 \u5206\u949F",
      "nurture.120s": "2 \u5206\u949F",
      "nurture.300s": "5 \u5206\u949F",
      "nurture.running": "\u517B\u53F7\u4E2D...",
      "nurture.completed": "\u517B\u53F7\u5B8C\u6210",
      "nurture.failed": "\u517B\u53F7\u5931\u8D25",
      "nurture.stopped": "\u517B\u53F7\u5DF2\u505C\u6B62",
      "nurture.noAccounts": "\u6682\u65E0\u8D26\u53F7\uFF0C\u8BF7\u5148\u6DFB\u52A0\u8D26\u53F7",
      // Settings
      "settings.title": "\u8BBE\u7F6E",
      "settings.aiConfig": "AI \u914D\u7F6E",
      "settings.aiConfigDesc": "\u914D\u7F6E AI \u7528\u4E8E\u751F\u6210\u81EA\u7136\u7684\u56DE\u590D\u5185\u5BB9\u3002\u586B\u5199\u4F60\u4F7F\u7528\u7684 AI \u670D\u52A1\u7684 API Key\u3002",
      "settings.defaultProvider": "\u9ED8\u8BA4 AI Provider",
      "settings.model": "\u6A21\u578B",
      "settings.refreshModels": "\u5237\u65B0\u6A21\u578B\u5217\u8868",
      "settings.modelHint": "\u8F93\u5165 API Key \u540E\u70B9\u51FB\u5237\u65B0\u83B7\u53D6\u6700\u65B0\u6A21\u578B\u5217\u8868",
      "settings.saveAI": "\u4FDD\u5B58 AI \u8BBE\u7F6E",
      "settings.testConnection": "\u6D4B\u8BD5\u8FDE\u63A5",
      "settings.browser": "\u6D4F\u89C8\u5668 (Unzoo)",
      "settings.browserProfile": "\u6D4F\u89C8\u5668 Profile",
      "settings.browserProfileDesc": "\u9009\u62E9\u7528\u4E8E\u53D1\u5E03\u5185\u5BB9\u7684\u6D4F\u89C8\u5668 Profile\uFF0C\u4E0B\u6B21\u4F1A\u81EA\u52A8\u8FDE\u63A5\u6B64 Profile",
      "settings.selectProfile": "-- \u9009\u62E9 Profile --",
      "settings.connect": "\u8FDE\u63A5",
      "settings.connected": "\u5DF2\u8FDE\u63A5",
      "settings.unzooPath": "Unzoo \u8DEF\u5F84",
      "settings.notFound": "\u672A\u627E\u5230",
      "settings.language": "\u8BED\u8A00\u8BBE\u7F6E",
      "settings.languageDesc": "\u9009\u62E9\u754C\u9762\u663E\u793A\u8BED\u8A00",
      "settings.chinese": "\u7B80\u4F53\u4E2D\u6587",
      "settings.english": "English",
      "settings.scheduler": "\u8C03\u5EA6\u5668",
      "settings.schedulerMode": "\u6A21\u5F0F",
      "settings.roundRobin": "\u8F6E\u8BE2",
      "settings.weighted": "\u6743\u91CD",
      "settings.priority": "\u4F18\u5148\u7EA7",
      "settings.smart": "\u667A\u80FD (AI)",
      "settings.interval": "\u95F4\u9694 (\u5206\u949F)",
      "settings.maxDailyPosts": "\u6BCF\u65E5\u6700\u5927\u53D1\u5E03\u6570",
      "settings.saveScheduler": "\u4FDD\u5B58\u8C03\u5EA6\u5668\u8BBE\u7F6E",
      "settings.proxyPool": "\u4EE3\u7406\u6C60",
      "settings.addProxy": "+ \u6DFB\u52A0\u4EE3\u7406",
      "settings.proxyDesc": "\u7BA1\u7406\u591A\u8D26\u53F7\u64CD\u4F5C\u7684\u4EE3\u7406\u3002\u6BCF\u4E2A\u8D26\u53F7\u53EF\u4EE5\u4F7F\u7528\u4E0D\u540C\u7684\u4EE3\u7406\u3002",
      "settings.total": "\u603B\u8BA1",
      "settings.active": "\u6D3B\u8DC3",
      "settings.inUse": "\u4F7F\u7528\u4E2D",
      "settings.failed": "\u5931\u8D25",
      "settings.bulkImport": "\u6279\u91CF\u5BFC\u5165",
      "settings.bulkImportHint": "\u6BCF\u884C\u4E00\u4E2A: protocol://ip:port \u6216 protocol://user:pass@ip:port",
      "settings.importProxies": "\u5BFC\u5165\u4EE3\u7406",
      "settings.noProxies": "\u6682\u65E0\u914D\u7F6E\u4EE3\u7406\u3002\u6DFB\u52A0\u4E00\u4E2A\u4EE3\u7406\u5F00\u59CB\u4F7F\u7528\u3002",
      "settings.scheduledJobs": "\u23F0 \u5B9A\u65F6\u4EFB\u52A1 (Unzoo)",
      "settings.addJob": "+ \u6DFB\u52A0\u4EFB\u52A1",
      "settings.jobsDesc": "\u4F7F\u7528 Unzoo \u8C03\u5EA6\u5668 API \u914D\u7F6E\u81EA\u52A8\u5316\u4EFB\u52A1\u3002",
      // Common
      "common.save": "\u4FDD\u5B58",
      "common.cancel": "\u53D6\u6D88",
      "common.delete": "\u5220\u9664",
      "common.edit": "\u7F16\u8F91",
      "common.refresh": "\u5237\u65B0",
      "common.loading": "\u52A0\u8F7D\u4E2D...",
      "common.error": "\u9519\u8BEF",
      "common.success": "\u6210\u529F",
      "common.confirm": "\u786E\u8BA4",
      "common.viewAll": "\u67E5\u770B\u5168\u90E8",
      "common.quickPublish": "\u5FEB\u901F\u53D1\u5E03",
      // Campaigns
      "campaigns.title": "\u63A8\u5E7F\u6D3B\u52A8",
      "campaigns.create": "+ \u521B\u5EFA\u6D3B\u52A8",
      "campaigns.active": "\u8FDB\u884C\u4E2D",
      "campaigns.scheduled": "\u5DF2\u8BA1\u5212",
      "campaigns.completed": "\u5DF2\u5B8C\u6210",
      "campaigns.totalTasks": "\u603B\u4EFB\u52A1\u6570",
      "campaigns.noCampaigns": "\u6682\u65E0\u6D3B\u52A8\uFF0C\u521B\u5EFA\u7B2C\u4E00\u4E2A\u8425\u9500\u6D3B\u52A8\u5F00\u59CB\u53D1\u5E03\u5427",
      "campaigns.createCampaign": "\u521B\u5EFA\u6D3B\u52A8",
      "campaigns.allStatus": "\u6240\u6709\u72B6\u6001",
      "campaigns.running": "\u8FD0\u884C\u4E2D",
      "campaigns.paused": "\u5DF2\u6682\u505C",
      "campaigns.draft": "\u8349\u7A3F",
      "campaigns.allPlatforms": "\u6240\u6709\u5E73\u53F0",
      // Products
      "products.title": "\u4EA7\u54C1\u7BA1\u7406",
      "products.add": "+ \u6DFB\u52A0\u4EA7\u54C1",
      "products.selectAll": "\u5168\u9009",
      "products.clear": "\u6E05\u9664",
      "products.noSelection": "\u672A\u9009\u62E9",
      "products.publishSelected": "\u53D1\u5E03\u9009\u4E2D",
      "products.noProducts": "\u6682\u65E0\u4EA7\u54C1",
      "products.addFirst": "\u6DFB\u52A0\u7B2C\u4E00\u4E2A\u4EA7\u54C1\u5F00\u59CB\u5168\u7403\u8425\u9500",
      // Publish
      "publish.title": "\u53D1\u5E03\u5185\u5BB9",
      "publish.refreshLimits": "\u{1F504} \u5237\u65B0\u9650\u5236",
      "publish.platformStatus": "\u{1F4CA} \u5E73\u53F0\u53D1\u5E03\u72B6\u6001",
      "publish.statusHint": "\u9632\u5C01\u9650\u5236\u4FDD\u62A4\u4F60\u7684\u8D26\u53F7\u3002\u7EFF\u8272 = \u53EF\u53D1\u5E03\uFF0C\u9EC4\u8272 = \u7B49\u5F85\uFF0C\u7EA2\u8272 = \u8FBE\u5230\u6BCF\u65E5\u9650\u5236\u3002",
      "publish.selectProducts": "\u9009\u62E9\u4EA7\u54C1",
      "publish.platforms": "\u5E73\u53F0",
      "publish.languages": "\u8BED\u8A00",
      "publish.generate": "\u751F\u6210\u5185\u5BB9",
      "publish.preview": "\u5185\u5BB9\u9884\u89C8",
      "publish.previewHint": "\u9009\u62E9\u4EA7\u54C1\u5E76\u751F\u6210\u5185\u5BB9\u8FDB\u884C\u9884\u89C8",
      "publish.simulate": "\u6A21\u62DF",
      "publish.publishAll": "\u53D1\u5E03\u5168\u90E8",
      "publish.selected": "\u5DF2\u9009\u62E9",
      // Articles
      "articles.title": "\u6587\u7AE0\u7BA1\u7406 (\u8F6F\u6587)",
      "articles.new": "+ \u65B0\u5EFA\u6587\u7AE0",
      "articles.selectProduct": "\u9009\u62E9\u4EA7\u54C1",
      "articles.chooseProduct": "\u9009\u62E9\u4E00\u4E2A\u4EA7\u54C1...",
      "articles.articleType": "\u6587\u7AE0\u7C7B\u578B",
      "articles.tutorial": "\u6559\u7A0B/\u6307\u5357",
      "articles.tutorialDesc": "\u4F7F\u7528\u65B9\u6CD5\uFF0C\u5206\u6B65\u6307\u5357",
      "articles.comparison": "\u5BF9\u6BD4\u8BC4\u6D4B",
      "articles.comparisonDesc": "\u4E0E\u7ADE\u54C1\u5BF9\u6BD4",
      "articles.problemSolving": "\u95EE\u9898\u89E3\u51B3",
      "articles.problemSolvingDesc": "\u75DB\u70B9 \u2192 \u89E3\u51B3\u65B9\u6848",
      "articles.story": "\u6545\u4E8B/\u6848\u4F8B",
      "articles.storyDesc": "\u7528\u6237\u6545\u4E8B\uFF0C\u4F7F\u7528\u573A\u666F",
      "articles.listicle": "\u6E05\u5355\u6587\u7AE0",
      "articles.listicleDesc": "Top 10, \u6700\u4F73 X \u5DE5\u5177",
      "articles.targetPlatforms": "\u76EE\u6807\u5E73\u53F0",
      "articles.seoKeywords": "SEO \u5173\u952E\u8BCD",
      "articles.tone": "\u8BED\u8C03\u98CE\u683C",
      "articles.professional": "\u4E13\u4E1A",
      "articles.casual": "\u8F7B\u677E\u53CB\u597D",
      "articles.technical": "\u6280\u672F",
      "articles.storytelling": "\u53D9\u4E8B",
      "articles.generateArticles": "\u751F\u6210\u6587\u7AE0",
      "articles.generatedArticles": "\u751F\u6210\u7684\u6587\u7AE0",
      "articles.generateHint": "\u9009\u62E9\u4EA7\u54C1\u5E76\u751F\u6210\u6587\u7AE0",
      "articles.copy": "\u590D\u5236",
      "articles.addToQueue": "\u52A0\u5165\u961F\u5217",
      "articles.savedArticles": "\u5DF2\u4FDD\u5B58\u7684\u6587\u7AE0",
      "articles.noSavedArticles": "\u6682\u65E0\u5DF2\u4FDD\u5B58\u7684\u6587\u7AE0",
      // Engage
      "engage.title": "\u{1F4AC} \u4E92\u52A8\u8425\u9500 (\u56DE\u590D\u7CFB\u7EDF)",
      "engage.addKeyword": "+ \u6DFB\u52A0\u5173\u952E\u8BCD",
      "engage.replyStatus": "\u{1F4CA} \u56DE\u590D\u72B6\u6001",
      "engage.replyHint": "\u901A\u8FC7\u56DE\u590D\u8FDB\u884C\u81EA\u7136\u4E92\u52A8\u6BD4\u53D1\u5E16\u66F4\u5B89\u5168\u6709\u6548\u3002",
      "engage.monitorKeywords": "\u{1F50D} \u76D1\u63A7\u5173\u952E\u8BCD",
      "engage.keywordsHint": "\u6211\u4EEC\u4F1A\u627E\u5230\u5339\u914D\u8FD9\u4E9B\u5173\u952E\u8BCD\u7684\u5E16\u5B50\u5E76\u5EFA\u8BAE\u56DE\u590D\u3002",
      "engage.noKeywords": "\u6682\u65E0\u5173\u952E\u8BCD\u3002\u6DFB\u52A0\u5173\u952E\u8BCD\u5F00\u59CB\u53D1\u73B0\u76F8\u5173\u5E16\u5B50\u3002",
      "engage.discoveredPosts": "\u{1F4EC} \u53D1\u73B0\u7684\u5E16\u5B50",
      "engage.discoverNow": "\u{1F50D} \u7ACB\u5373\u53D1\u73B0",
      "engage.discoverHint": '\u6DFB\u52A0\u5173\u952E\u8BCD\u5E76\u70B9\u51FB"\u7ACB\u5373\u53D1\u73B0"\u6765\u67E5\u627E\u76F8\u5173\u5E16\u5B50\u3002',
      // Tasks
      "tasks.title": "\u4EFB\u52A1\u961F\u5217",
      "tasks.clearCompleted": "\u6E05\u9664\u5DF2\u5B8C\u6210",
      "tasks.retryFailed": "\u91CD\u8BD5\u5931\u8D25",
      "tasks.pending": "\u5F85\u5904\u7406",
      "tasks.running": "\u8FD0\u884C\u4E2D",
      "tasks.completed": "\u5DF2\u5B8C\u6210",
      "tasks.noTasks": "\u6682\u65E0\u4EFB\u52A1",
      "tasks.tasksHint": "\u53D1\u5E03\u6216\u56DE\u590D\u65F6\u4F1A\u5728\u8FD9\u91CC\u663E\u793A\u4EFB\u52A1",
      // Stats
      "stats.title": "\u7EDF\u8BA1\u5206\u6790",
      "stats.export": "\u{1F4E5} \u5BFC\u51FA",
      "stats.last7days": "\u6700\u8FD1 7 \u5929",
      "stats.last30days": "\u6700\u8FD1 30 \u5929",
      "stats.last90days": "\u6700\u8FD1 90 \u5929",
      "stats.lastYear": "\u6700\u8FD1\u4E00\u5E74",
      "stats.allTime": "\u6240\u6709\u65F6\u95F4",
      "stats.totalPosts": "\u603B\u53D1\u5E03\u6570",
      "stats.totalViews": "\u603B\u6D4F\u89C8\u91CF",
      "stats.engagements": "\u4E92\u52A8\u6570",
      "stats.engagementRate": "\u4E92\u52A8\u7387",
      "stats.activityTrend": "\u{1F4CA} \u6D3B\u52A8\u8D8B\u52BF",
      "stats.posts": "\u53D1\u5E03",
      "stats.views": "\u6D4F\u89C8",
      "stats.platformPerformance": "\u{1F310} \u5E73\u53F0\u8868\u73B0",
      "stats.contentBreakdown": "\u{1F4D1} \u5185\u5BB9\u7C7B\u578B\u5206\u5E03",
      "stats.articles": "\u6587\u7AE0",
      "stats.replies": "\u56DE\u590D",
      "stats.reposts": "\u8F6C\u53D1",
      "stats.comments": "\u8BC4\u8BBA",
      // Messages
      "msg.profileBound": "\u5DF2\u7ED1\u5B9A Profile",
      "msg.profileUnbound": "\u5DF2\u89E3\u7ED1\uFF0C\u5C06\u4F7F\u7528\u5168\u5C40 Profile",
      "msg.bindFailed": "\u7ED1\u5B9A\u5931\u8D25",
      "msg.nurtureStarted": "\u5F00\u59CB\u517B\u53F7",
      "msg.nurtureCompleted": "\u517B\u53F7\u5B8C\u6210",
      "msg.nurtureFailed": "\u517B\u53F7\u5931\u8D25",
      "msg.selectProfileFirst": "\u8BF7\u5148\u9009\u62E9\u4E00\u4E2A Profile",
      "msg.connecting": "\u6B63\u5728\u8FDE\u63A5...",
      "msg.profilesRefreshed": "Profiles \u5DF2\u5237\u65B0",
      "msg.connectionFailed": "\u8FDE\u63A5\u5931\u8D25",
      "msg.browserMode": "\u6D4F\u89C8\u5668\u6A21\u5F0F - \u529F\u80FD\u53D7\u9650\u3002\u8BF7\u4F7F\u7528 Tauri \u684C\u9762\u5E94\u7528\u4EE5\u83B7\u53D6\u5B8C\u6574\u529F\u80FD\u3002",
      "msg.failedToLoad": "\u52A0\u8F7D\u6570\u636E\u5931\u8D25",
      "msg.enterCampaignName": "\u8BF7\u8F93\u5165\u6D3B\u52A8\u540D\u79F0",
      "msg.selectProduct": "\u8BF7\u9009\u62E9\u4E00\u4E2A\u4EA7\u54C1",
      "msg.selectPlatform": "\u8BF7\u9009\u62E9\u81F3\u5C11\u4E00\u4E2A\u5E73\u53F0",
      "msg.campaignStarted": "\u6D3B\u52A8\u5DF2\u5F00\u59CB\uFF01",
      "msg.campaignPaused": "\u6D3B\u52A8\u5DF2\u6682\u505C",
      "msg.campaignDeleted": "\u6D3B\u52A8\u5DF2\u5220\u9664",
      "msg.productAdded": "\u4EA7\u54C1\u6DFB\u52A0\u6210\u529F",
      "msg.productDeleted": "\u4EA7\u54C1\u5DF2\u5220\u9664",
      "msg.enterUrl": "\u8BF7\u8F93\u5165 URL",
      "msg.urlAnalyzed": "URL \u5206\u6790\u5B8C\u6210",
      "msg.nameUrlRequired": "\u540D\u79F0\u548C URL \u662F\u5FC5\u586B\u9879",
      "msg.gmailConnected": "Gmail \u8FDE\u63A5\u6210\u529F\uFF01",
      "msg.accountAdded": "\u8D26\u53F7\u5DF2\u6DFB\u52A0",
      "msg.accountDeleted": "\u8D26\u53F7\u5DF2\u5220\u9664",
      "msg.platformUsernameRequired": "\u5E73\u53F0\u548C\u7528\u6237\u540D\u662F\u5FC5\u586B\u9879",
      "msg.syncCompleted": "\u540C\u6B65\u5B8C\u6210",
      "msg.profileCreated": "Profile \u5DF2\u521B\u5EFA",
      "msg.proxyUpdated": "\u4EE3\u7406\u5DF2\u66F4\u65B0",
      "msg.proxyAdded": "\u4EE3\u7406\u6DFB\u52A0\u6210\u529F",
      "msg.proxyDeleted": "\u4EE3\u7406\u5DF2\u5220\u9664",
      "msg.testingProxy": "\u6B63\u5728\u6D4B\u8BD5\u4EE3\u7406...",
      "msg.jobCreated": "\u5B9A\u65F6\u4EFB\u52A1\u5DF2\u521B\u5EFA",
      "msg.jobDeleted": "\u4EFB\u52A1\u5DF2\u5220\u9664",
      "msg.noContentToPublish": "\u6CA1\u6709\u5185\u5BB9\u53EF\u53D1\u5E03",
      "msg.allPublished": "\u5168\u90E8\u53D1\u5E03\u5B8C\u6210\uFF01",
      "msg.publishCancelled": "\u53D1\u5E03\u5DF2\u53D6\u6D88",
      "msg.keywordAdded": "\u5173\u952E\u8BCD\u5DF2\u6DFB\u52A0",
      "msg.keywordDeleted": "\u5173\u952E\u8BCD\u5DF2\u5220\u9664",
      "msg.addKeywordsFirst": "\u8BF7\u5148\u6DFB\u52A0\u5173\u952E\u8BCD",
      "msg.replyGenerated": "\u56DE\u590D\u5DF2\u751F\u6210",
      "msg.replySent": "\u56DE\u590D\u53D1\u9001\u6210\u529F\uFF01",
      "msg.pleaseWriteReply": "\u8BF7\u8F93\u5165\u56DE\u590D\u5185\u5BB9",
      "msg.aiSettingsSaved": "AI \u8BBE\u7F6E\u5DF2\u4FDD\u5B58",
      "msg.schedulerSettingsSaved": "\u8C03\u5EA6\u5668\u8BBE\u7F6E\u5DF2\u4FDD\u5B58",
      "msg.selectLanguage": "\u8BF7\u9009\u62E9\u81F3\u5C11\u4E00\u79CD\u8BED\u8A00",
      "msg.articleCopied": "\u6587\u7AE0\u5DF2\u590D\u5236\u5230\u526A\u8D34\u677F",
      "msg.articlesQueued": "\u6587\u7AE0\u5DF2\u52A0\u5165\u4EFB\u52A1\u961F\u5217",
      "msg.taskAdded": "\u4EFB\u52A1\u5DF2\u52A0\u5165\u961F\u5217",
      "msg.clearedTasks": "\u5DF2\u6E05\u9664\u5B8C\u6210\u7684\u4EFB\u52A1",
      "msg.comingSoon": "\u529F\u80FD\u5373\u5C06\u63A8\u51FA",
      "msg.simulationMode": "\u6A21\u62DF\u6A21\u5F0F - \u4E0D\u4F1A\u5B9E\u9645\u53D1\u5E03",
      "msg.usingDefaultModels": "\u4F7F\u7528\u9ED8\u8BA4\u6A21\u578B\u5217\u8868",
      "msg.apiKeyNotSaved": "API Key \u672A\u4FDD\u5B58\uFF0C\u8BF7\u5148\u4FDD\u5B58\u8BBE\u7F6E",
      "msg.networkError": "\u7F51\u7EDC\u9519\u8BEF",
      "msg.statsExported": "\u7EDF\u8BA1\u6570\u636E\u5BFC\u51FA\u6210\u529F\uFF01",
      "msg.account": "\u8D26\u53F7",
      "msg.failed": "\u5931\u8D25"
    },
    en: {
      // Navigation
      "nav.dashboard": "Dashboard",
      "nav.campaigns": "Campaigns",
      "nav.products": "Products",
      "nav.publish": "Publish",
      "nav.articles": "Articles",
      "nav.engage": "Engage",
      "nav.accounts": "Accounts",
      "nav.tasks": "Tasks",
      "nav.statistics": "Statistics",
      "nav.settings": "Settings",
      // Dashboard
      "dashboard.title": "Dashboard",
      "dashboard.activeTasks": "Active Tasks",
      "dashboard.publishedToday": "Published Today",
      "dashboard.accountHealth": "Account Health",
      "dashboard.successRate": "Success Rate",
      "dashboard.activeCampaigns": "Active Campaigns",
      "dashboard.newCampaign": "+ New Campaign",
      "dashboard.noCampaigns": "No active campaigns. Create one to start publishing!",
      "dashboard.recentActivity": "Recent Activity",
      "dashboard.noActivity": "No recent activity",
      "dashboard.platformHealth": "Platform Health",
      "dashboard.quickActions": "Quick Actions",
      "dashboard.addProduct": "Add Product",
      "dashboard.addAccount": "Add Account",
      "dashboard.createContent": "Create Content",
      "dashboard.startEngage": "Start Engage",
      // Accounts
      "accounts.title": "Platform Accounts",
      "accounts.addAccount": "+ Add Account",
      "accounts.overallHealth": "Overall Health",
      "accounts.active": "Active",
      "accounts.warning": "Warning",
      "accounts.banned": "Banned",
      "accounts.warming": "Warming",
      "accounts.autoRegister": "Auto-Register (via Unzoo)",
      "accounts.checking": "Checking...",
      "accounts.connectGmail": "Connect Gmail",
      "accounts.selectPlatforms": "Select Platforms",
      "accounts.phone": "phone",
      "accounts.googleOAuth": "Google OAuth",
      "accounts.selectAll": "Select All",
      "accounts.deselectAll": "Deselect All",
      "accounts.autoLogin": "Auto-Login/Register Selected",
      "accounts.syncAll": "Sync All",
      "accounts.syncAllHint": `"Sync All" will check which platforms you're already logged into in Unzoo browser`,
      "accounts.existingAccounts": "Existing Accounts",
      "accounts.delete": "Delete",
      "accounts.useGlobalProfile": "-- Use Global Profile --",
      "accounts.createProfile": "Create Profile",
      "accounts.noAccounts": "No accounts configured",
      "accounts.autoRegisterHint": "Use auto-register or add accounts manually",
      // Nurturing
      "nurture.title": "Account Nurturing",
      "nurture.description": "Simulate normal user browsing to improve account weight",
      "nurture.quickNurture": "Quick Nurture",
      "nurture.startNurture": "Start Nurturing",
      "nurture.stopNurture": "Stop Nurturing",
      "nurture.totalTime": "Total Nurturing Time",
      "nurture.lastNurture": "Last Nurtured",
      "nurture.hours": "hours",
      "nurture.days": "days",
      "nurture.seconds": "seconds",
      "nurture.selectAccount": "Select account to nurture",
      "nurture.duration": "Duration",
      "nurture.30s": "30 seconds",
      "nurture.60s": "1 minute",
      "nurture.120s": "2 minutes",
      "nurture.300s": "5 minutes",
      "nurture.running": "Nurturing...",
      "nurture.completed": "Nurturing completed",
      "nurture.failed": "Nurturing failed",
      "nurture.stopped": "Nurturing stopped",
      "nurture.noAccounts": "No accounts yet, please add accounts first",
      // Settings
      "settings.title": "Settings",
      "settings.aiConfig": "AI Configuration",
      "settings.aiConfigDesc": "Configure AI for generating natural responses. Enter API Key for your AI service.",
      "settings.defaultProvider": "Default AI Provider",
      "settings.model": "Model",
      "settings.refreshModels": "Refresh Model List",
      "settings.modelHint": "Enter API Key then click refresh to get latest model list",
      "settings.saveAI": "Save AI Settings",
      "settings.testConnection": "Test Connection",
      "settings.browser": "Browser (Unzoo)",
      "settings.browserProfile": "Browser Profile",
      "settings.browserProfileDesc": "Select browser profile for publishing, will auto-connect next time",
      "settings.selectProfile": "-- Select Profile --",
      "settings.connect": "Connect",
      "settings.connected": "Connected",
      "settings.unzooPath": "Unzoo Path",
      "settings.notFound": "Not found",
      "settings.language": "Language",
      "settings.languageDesc": "Select interface language",
      "settings.chinese": "\u7B80\u4F53\u4E2D\u6587",
      "settings.english": "English",
      "settings.scheduler": "Scheduler",
      "settings.schedulerMode": "Mode",
      "settings.roundRobin": "Round Robin",
      "settings.weighted": "Weighted",
      "settings.priority": "Priority",
      "settings.smart": "Smart (AI)",
      "settings.interval": "Interval (minutes)",
      "settings.maxDailyPosts": "Max Daily Posts",
      "settings.saveScheduler": "Save Scheduler Settings",
      "settings.proxyPool": "Proxy Pool",
      "settings.addProxy": "+ Add Proxy",
      "settings.proxyDesc": "Manage proxies for multi-account operations. Each account can use a different proxy.",
      "settings.total": "Total",
      "settings.active": "Active",
      "settings.inUse": "In Use",
      "settings.failed": "Failed",
      "settings.bulkImport": "Bulk Import",
      "settings.bulkImportHint": "One per line: protocol://ip:port or protocol://user:pass@ip:port",
      "settings.importProxies": "Import Proxies",
      "settings.noProxies": "No proxies configured. Add a proxy to get started.",
      "settings.scheduledJobs": "\u23F0 Scheduled Jobs (Unzoo)",
      "settings.addJob": "+ Add Job",
      "settings.jobsDesc": "Configure automated tasks using Unzoo's scheduler API.",
      // Common
      "common.save": "Save",
      "common.cancel": "Cancel",
      "common.delete": "Delete",
      "common.edit": "Edit",
      "common.refresh": "Refresh",
      "common.loading": "Loading...",
      "common.error": "Error",
      "common.success": "Success",
      "common.confirm": "Confirm",
      "common.viewAll": "View All",
      "common.quickPublish": "Quick Publish",
      // Campaigns
      "campaigns.title": "Campaigns",
      "campaigns.create": "+ Create Campaign",
      "campaigns.active": "Active",
      "campaigns.scheduled": "Scheduled",
      "campaigns.completed": "Completed",
      "campaigns.totalTasks": "Total Tasks",
      "campaigns.noCampaigns": "No campaigns yet. Create your first marketing campaign to start publishing",
      "campaigns.createCampaign": "Create Campaign",
      "campaigns.allStatus": "All Status",
      "campaigns.running": "Running",
      "campaigns.paused": "Paused",
      "campaigns.draft": "Draft",
      "campaigns.allPlatforms": "All Platforms",
      // Products
      "products.title": "Products",
      "products.add": "+ Add Product",
      "products.selectAll": "Select All",
      "products.clear": "Clear",
      "products.noSelection": "No selection",
      "products.publishSelected": "Publish Selected",
      "products.noProducts": "No products yet",
      "products.addFirst": "Add your first product to start marketing globally",
      // Publish
      "publish.title": "Publish Content",
      "publish.refreshLimits": "\u{1F504} Refresh Limits",
      "publish.platformStatus": "\u{1F4CA} Platform Publishing Status",
      "publish.statusHint": "Anti-ban limits protect your accounts. Green = can post, Yellow = wait, Red = daily limit reached.",
      "publish.selectProducts": "Select Products",
      "publish.platforms": "Platforms",
      "publish.languages": "Languages",
      "publish.generate": "Generate Content",
      "publish.preview": "Content Preview",
      "publish.previewHint": "Select a product and generate content to preview",
      "publish.simulate": "Simulate",
      "publish.publishAll": "Publish All",
      "publish.selected": "selected",
      // Articles
      "articles.title": "Articles",
      "articles.new": "+ New Article",
      "articles.selectProduct": "Select Product",
      "articles.chooseProduct": "Choose a product...",
      "articles.articleType": "Article Type",
      "articles.tutorial": "Tutorial/Guide",
      "articles.tutorialDesc": "How to use, step-by-step guide",
      "articles.comparison": "Comparison",
      "articles.comparisonDesc": "Compare with alternatives",
      "articles.problemSolving": "Problem-Solving",
      "articles.problemSolvingDesc": "Pain point \u2192 Solution",
      "articles.story": "Story/Case",
      "articles.storyDesc": "User story, use case",
      "articles.listicle": "Listicle",
      "articles.listicleDesc": "Top 10, Best X tools",
      "articles.targetPlatforms": "Target Platforms",
      "articles.seoKeywords": "SEO Keywords",
      "articles.tone": "Tone",
      "articles.professional": "Professional",
      "articles.casual": "Casual/Friendly",
      "articles.technical": "Technical",
      "articles.storytelling": "Storytelling",
      "articles.generateArticles": "Generate Articles",
      "articles.generatedArticles": "Generated Articles",
      "articles.generateHint": "Select a product and generate articles",
      "articles.copy": "Copy",
      "articles.addToQueue": "Add to Queue",
      "articles.savedArticles": "Saved Articles",
      "articles.noSavedArticles": "No saved articles yet",
      // Engage
      "engage.title": "\u{1F4AC} Engage (Reply System)",
      "engage.addKeyword": "+ Add Keyword",
      "engage.replyStatus": "\u{1F4CA} Reply Status",
      "engage.replyHint": "Natural engagement through replies is safer and more effective than posting.",
      "engage.monitorKeywords": "\u{1F50D} Monitoring Keywords",
      "engage.keywordsHint": "We'll find posts matching these keywords and suggest replies.",
      "engage.noKeywords": "No keywords yet. Add keywords to start discovering relevant posts.",
      "engage.discoveredPosts": "\u{1F4EC} Discovered Posts",
      "engage.discoverNow": "\u{1F50D} Discover Now",
      "engage.discoverHint": 'Add keywords and click "Discover Now" to find relevant posts.',
      // Tasks
      "tasks.title": "Tasks",
      "tasks.clearCompleted": "Clear Completed",
      "tasks.retryFailed": "Retry Failed",
      "tasks.pending": "pending",
      "tasks.running": "running",
      "tasks.completed": "completed",
      "tasks.noTasks": "No tasks yet",
      "tasks.tasksHint": "Tasks will appear here when you publish or reply",
      // Stats
      "stats.title": "Statistics & Analytics",
      "stats.export": "\u{1F4E5} Export",
      "stats.last7days": "Last 7 days",
      "stats.last30days": "Last 30 days",
      "stats.last90days": "Last 90 days",
      "stats.lastYear": "Last year",
      "stats.allTime": "All time",
      "stats.totalPosts": "Total Posts",
      "stats.totalViews": "Total Views",
      "stats.engagements": "Engagements",
      "stats.engagementRate": "Engagement Rate",
      "stats.activityTrend": "\u{1F4CA} Activity Trend",
      "stats.posts": "Posts",
      "stats.views": "Views",
      "stats.platformPerformance": "\u{1F310} Platform Performance",
      "stats.contentBreakdown": "\u{1F4D1} Content Type Breakdown",
      "stats.articles": "Articles",
      "stats.replies": "Replies",
      "stats.reposts": "Reposts",
      "stats.comments": "Comments",
      // Messages
      "msg.profileBound": "Profile bound",
      "msg.profileUnbound": "Profile unbound, will use global profile",
      "msg.bindFailed": "Bind failed",
      "msg.nurtureStarted": "Nurturing started",
      "msg.nurtureCompleted": "Nurturing completed",
      "msg.nurtureFailed": "Nurturing failed",
      "msg.selectProfileFirst": "Please select a profile first",
      "msg.connecting": "Connecting...",
      "msg.profilesRefreshed": "Profiles refreshed",
      "msg.connectionFailed": "Connection failed",
      "msg.browserMode": "Browser Mode - Features limited. Please use the Tauri desktop app for full functionality.",
      "msg.failedToLoad": "Failed to load data",
      "msg.enterCampaignName": "Please enter a campaign name",
      "msg.selectProduct": "Please select a product",
      "msg.selectPlatform": "Please select at least one platform",
      "msg.campaignStarted": "Campaign started!",
      "msg.campaignPaused": "Campaign paused",
      "msg.campaignDeleted": "Campaign deleted",
      "msg.productAdded": "Product added successfully",
      "msg.productDeleted": "Product deleted",
      "msg.enterUrl": "Please enter a URL",
      "msg.urlAnalyzed": "URL analyzed",
      "msg.nameUrlRequired": "Name and URL are required",
      "msg.gmailConnected": "Gmail connected successfully!",
      "msg.accountAdded": "Account added",
      "msg.accountDeleted": "Account deleted",
      "msg.platformUsernameRequired": "Platform and username are required",
      "msg.syncCompleted": "Sync completed",
      "msg.profileCreated": "Profile created",
      "msg.proxyUpdated": "Proxy updated",
      "msg.proxyAdded": "Proxy added successfully",
      "msg.proxyDeleted": "Proxy deleted",
      "msg.testingProxy": "Testing proxy...",
      "msg.jobCreated": "Scheduled job created",
      "msg.jobDeleted": "Job deleted",
      "msg.noContentToPublish": "No content to publish",
      "msg.allPublished": "All content published!",
      "msg.publishCancelled": "Publishing cancelled",
      "msg.keywordAdded": "Keyword added",
      "msg.keywordDeleted": "Keyword deleted",
      "msg.addKeywordsFirst": "Please add keywords first",
      "msg.replyGenerated": "Reply generated",
      "msg.replySent": "Reply sent successfully!",
      "msg.pleaseWriteReply": "Please write a reply",
      "msg.aiSettingsSaved": "AI settings saved",
      "msg.schedulerSettingsSaved": "Scheduler settings saved",
      "msg.selectLanguage": "Select at least one language",
      "msg.articleCopied": "Article copied to clipboard",
      "msg.articlesQueued": "Articles added to task queue",
      "msg.taskAdded": "Task added to queue",
      "msg.clearedTasks": "Cleared completed tasks",
      "msg.comingSoon": "Coming soon",
      "msg.simulationMode": "Simulation mode - no actual posts",
      "msg.usingDefaultModels": "Using default model list",
      "msg.apiKeyNotSaved": "API Key not saved, please save settings first",
      "msg.networkError": "Network error",
      "msg.statsExported": "Statistics exported successfully!",
      "msg.account": "Account",
      "msg.failed": "failed"
    }
  };
  function t(key) {
    return translations[currentLanguage][key] || translations["en"][key] || key;
  }
  function updateAllTranslations() {
    document.querySelectorAll("[data-i18n]").forEach((el) => {
      const key = el.getAttribute("data-i18n");
      if (key) {
        el.textContent = t(key);
      }
    });
    document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      const key = el.getAttribute("data-i18n-placeholder");
      if (key) {
        el.placeholder = t(key);
      }
    });
    const navMapping = {
      "dashboard": "nav.dashboard",
      "campaigns": "nav.campaigns",
      "products": "nav.products",
      "publish": "nav.publish",
      "articles": "nav.articles",
      "engage": "nav.engage",
      "accounts": "nav.accounts",
      "tasks": "nav.tasks",
      "stats": "nav.statistics",
      "settings": "nav.settings"
    };
    document.querySelectorAll(".nav-item").forEach((item) => {
      const page = item.dataset.page;
      if (page && navMapping[page]) {
        const textEl = item.querySelector(".nav-text");
        if (textEl) {
          textEl.textContent = t(navMapping[page]);
        }
      }
    });
  }
  function setLanguage(lang) {
    currentLanguage = lang;
    localStorage.setItem("unmarket_language", lang);
    updateAllTranslations();
    if (typeof renderCurrentPage === "function") {
      renderCurrentPage();
    }
  }
  function loadSavedLanguage() {
    const saved = localStorage.getItem("unmarket_language");
    if (saved && (saved === "zh" || saved === "en")) {
      currentLanguage = saved;
    }
  }
  var isTauriEnv = false;
  try {
    isTauriEnv = !!window.__TAURI_INTERNALS__;
  } catch {
    isTauriEnv = false;
  }
  async function invoke2(cmd, args) {
    if (!isTauriEnv) {
      console.warn(`[Browser Mode] invoke('${cmd}') not available outside Tauri`);
      throw new Error("Please use the Tauri desktop app, not browser");
    }
    return invoke(cmd, args);
  }
  var currentPage = "products";
  var products = [];
  var accounts = [];
  var generatedContents = [];
  var selectedProductIds = /* @__PURE__ */ new Set();
  var tasks = [];
  var taskRunning = false;
  var generatedArticles = [];
  var currentArticleIndex = 0;
  var savedArticles = [];
  var browserProfiles = [];
  var scheduledJobs = [];
  var defaultAiProviders = {
    gemini: { name: "Google Gemini", models: ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"] },
    openai: { name: "OpenAI", models: ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo"] },
    deepseek: { name: "DeepSeek", models: ["deepseek-chat", "deepseek-coder"] },
    qwen: { name: "\u963F\u91CC\u5343\u95EE", models: ["qwen-turbo", "qwen-plus", "qwen-max"] }
  };
  var aiProviders = { ...defaultAiProviders };
  document.addEventListener("DOMContentLoaded", async () => {
    console.log("Tauri app initializing...");
    loadSavedLanguage();
    updateAllTranslations();
    if (!isTauriEnv) {
      showBrowserModeWarning();
    }
    initNavigation();
    initModals();
    initTabs();
    initCampaignEvents();
    initProxyEvents();
    await loadInitialData();
    checkBrowserStatus();
  });
  function showBrowserModeWarning() {
    const banner = document.createElement("div");
    banner.style.cssText = "position:fixed;top:0;left:0;right:0;background:#ff5722;color:white;padding:10px;text-align:center;z-index:9999;font-size:14px;";
    banner.innerHTML = '\u26A0\uFE0F Browser Mode - Features limited. Please use the Tauri desktop app for full functionality. <button onclick="this.parentElement.remove()" style="margin-left:10px;padding:2px 8px;cursor:pointer;">\u2715</button>';
    document.body.prepend(banner);
  }
  function initNavigation() {
    document.querySelectorAll(".nav-item").forEach((item) => {
      item.addEventListener("click", (e) => {
        e.preventDefault();
        const page = item.dataset.page;
        if (page) navigateTo(page);
      });
    });
  }
  function navigateTo(page) {
    document.querySelectorAll(".nav-item").forEach((item) => {
      item.classList.toggle("active", item.dataset.page === page);
    });
    document.querySelectorAll(".page").forEach((p) => {
      p.classList.toggle("active", p.id === `page-${page}`);
    });
    currentPage = page;
    switch (page) {
      case "dashboard":
        loadDashboard();
        break;
      case "campaigns":
        loadCampaigns();
        break;
      case "products":
        loadProducts();
        break;
      case "publish":
        loadPublishPage();
        break;
      case "articles":
        loadArticlesPage();
        break;
      case "engage":
        loadEngagePage();
        break;
      case "accounts":
        loadAccounts();
        break;
      case "tasks":
        loadTasksPage();
        break;
      case "content":
        loadContentPage();
        break;
      case "metrics":
        loadMetricsPage();
        break;
      case "personas":
        loadPersonasPage();
        break;
      case "marketplaces":
        loadMarketplacesPage();
        break;
      case "stats":
        loadStats();
        break;
      case "settings":
        loadSettings();
        break;
    }
  }
  window.navigateTo = navigateTo;
  function renderCurrentPage() {
    navigateTo(currentPage);
  }
  window.setLanguage = function(lang) {
    setLanguage(lang);
    const selector = document.getElementById("languageSelect");
    if (selector) selector.value = lang;
    showToast(lang === "zh" ? "\u8BED\u8A00\u5DF2\u5207\u6362\u4E3A\u4E2D\u6587" : "Language switched to English", "success");
  };
  function initModals() {
    document.querySelectorAll("[data-close]").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll(".modal").forEach((m) => m.classList.remove("active"));
      });
    });
    document.querySelectorAll(".modal").forEach((modal) => {
      modal.addEventListener("click", (e) => {
        if (e.target === modal) modal.classList.remove("active");
      });
    });
    document.getElementById("btnAddProduct")?.addEventListener("click", () => openModal("modalAddProduct"));
    document.getElementById("btnAddProductEmpty")?.addEventListener("click", () => openModal("modalAddProduct"));
    document.getElementById("btnSaveProduct")?.addEventListener("click", saveProduct);
    document.getElementById("btnSelectAllProducts")?.addEventListener("click", selectAllProducts);
    document.getElementById("btnClearSelection")?.addEventListener("click", clearProductSelection);
    document.getElementById("btnBatchPublish")?.addEventListener("click", batchPublishSelected);
    document.getElementById("btnSelectAllPublish")?.addEventListener("click", selectAllPublishProducts);
    document.getElementById("btnClearPublish")?.addEventListener("click", clearPublishProducts);
    document.getElementById("btnAnalyze")?.addEventListener("click", analyzeUrl);
    document.getElementById("btnAddAccount")?.addEventListener("click", () => openModal("modalAddAccount"));
    document.getElementById("btnAddAccountEmpty")?.addEventListener("click", () => openModal("modalAddAccount"));
    document.getElementById("btnSaveAccount")?.addEventListener("click", saveAccount);
    document.getElementById("btnSetupGmail")?.addEventListener("click", setupGmail);
    document.getElementById("btnAutoRegister")?.addEventListener("click", autoRegister);
    document.getElementById("btnSyncAll")?.addEventListener("click", syncAllPlatforms);
    document.getElementById("registerPlatforms")?.addEventListener("change", updateRegisterButton);
    document.getElementById("btnSelectAll")?.addEventListener("click", selectAllPlatforms);
    document.getElementById("btnDeselectAll")?.addEventListener("click", deselectAllPlatforms);
    document.getElementById("btnSelectAllPublishPlatforms")?.addEventListener("click", selectAllPublishPlatforms);
    document.getElementById("btnDeselectAllPublishPlatforms")?.addEventListener("click", deselectAllPublishPlatforms);
    document.getElementById("btnSelectAllCampaignPlatforms")?.addEventListener("click", selectAllCampaignPlatforms);
    document.getElementById("btnDeselectAllCampaignPlatforms")?.addEventListener("click", deselectAllCampaignPlatforms);
    document.getElementById("btnGenerate")?.addEventListener("click", generateContent);
    document.getElementById("btnSimulate")?.addEventListener("click", simulatePublish);
    document.getElementById("btnPublish")?.addEventListener("click", queuePublishTask);
    document.getElementById("btnClearCompleted")?.addEventListener("click", clearCompletedTasks);
    document.getElementById("btnCreateNurtureTask")?.addEventListener("click", openBatchNurtureModal);
    document.getElementById("btnGenerateArticle")?.addEventListener("click", generateArticles);
    document.getElementById("btnCopyArticle")?.addEventListener("click", copyCurrentArticle);
    document.getElementById("btnPublishArticle")?.addEventListener("click", queueArticleTask);
    initArticleTypeSelection();
    document.getElementById("btnRefreshStrategies")?.addEventListener("click", loadPublishStrategies);
    document.getElementById("btnSaveAI")?.addEventListener("click", saveAISettings);
    document.getElementById("btnTestAI")?.addEventListener("click", testAIConnection);
    document.getElementById("btnSaveScheduler")?.addEventListener("click", saveSchedulerSettings);
    document.getElementById("aiProvider")?.addEventListener("change", () => {
      populateDefaultModels();
      updateAIKeyVisibility();
    });
    document.getElementById("btnRefreshModels")?.addEventListener("click", refreshModels);
    document.getElementById("btnAddKeyword")?.addEventListener("click", () => openKeywordModal());
    document.getElementById("btnSaveKeyword")?.addEventListener("click", saveKeyword);
    document.getElementById("btnDiscoverNow")?.addEventListener("click", discoverPosts);
    document.getElementById("btnGenerateReply")?.addEventListener("click", generateReplyContent);
    document.getElementById("btnSendReply")?.addEventListener("click", sendReply);
    document.getElementById("btnSaveProxy")?.addEventListener("click", saveProxy);
    document.getElementById("btnAddSchedule")?.addEventListener("click", () => openModal("modalAddSchedule"));
    document.getElementById("btnSaveSchedule")?.addEventListener("click", createScheduledJob);
  }
  function openModal(id) {
    document.getElementById(id)?.classList.add("active");
  }
  function closeModal(id) {
    document.getElementById(id)?.classList.remove("active");
  }
  window.openModal = openModal;
  window.closeModal = closeModal;
  function initTabs() {
    document.querySelectorAll(".tabs").forEach((tabGroup) => {
      tabGroup.querySelectorAll(".tab").forEach((tab) => {
        tab.addEventListener("click", () => {
          tabGroup.querySelectorAll(".tab").forEach((t2) => t2.classList.remove("active"));
          tab.classList.add("active");
          const tabId = tab.dataset.tab;
          const parent = tabGroup.parentElement;
          parent?.querySelectorAll(".tab-content").forEach((content) => {
            content.classList.toggle("active", content.id === `tab-${tabId}`);
          });
        });
      });
    });
  }
  async function loadInitialData() {
    try {
      console.log("Loading initial data...");
      products = await invoke2("list_products");
      const providers = await invoke2("get_ai_providers");
      aiProviders = { ...defaultAiProviders, ...providers || {} };
      console.log("Products loaded:", products.length);
      await loadDashboard();
    } catch (error) {
      console.error("Failed to load initial data:", error);
      showToast(t("msg.failedToLoad"), "error");
    }
  }
  var dashWired = false;
  var dashPollTimer;
  async function loadDashboard() {
    if (!dashWired) {
      dashWired = true;
      wireAutopilot();
    }
    refreshConsole();
    if (dashPollTimer) clearInterval(dashPollTimer);
    dashPollTimer = window.setInterval(() => {
      if (currentPage === "dashboard") refreshConsole();
      else if (dashPollTimer) {
        clearInterval(dashPollTimer);
        dashPollTimer = void 0;
      }
    }, 5e3);
  }
  function wireAutopilot() {
    document.getElementById("btnAutopilot")?.addEventListener("click", async () => {
      try {
        const st = await invoke2("get_run_state");
        if (st.autopilot) {
          await invoke2("stop_engine");
          showToast("\u5DF2\u6682\u505C\u5168\u81EA\u52A8", "info");
        } else {
          await invoke2("set_run_option", { key: "reply_mode", value: "auto" });
          await invoke2("set_run_option", { key: "dry_run", value: "0" });
          try {
            await invoke2("start_engine");
          } catch (_e) {
          }
          showToast("\u5DF2\u5F00\u542F\u5168\u81EA\u52A8\uFF1A\u5F15\u64CE\u81EA\u52A8\u517B\u53F7\u2192\u53D1\u73B0\u2192\u56DE\u590D\uFF0C\u5168\u7A0B\u4E0D\u7528\u4F60\u786E\u8BA4", "success");
        }
      } catch (e) {
        showToast("" + e, "error");
      }
      setTimeout(refreshConsole, 500);
    });
    const knob = (id, key, get) => document.getElementById(id)?.addEventListener("change", async (ev) => {
      try {
        await invoke2("set_run_option", { key, value: get(ev.target) });
        showToast("\u5DF2\u4FDD\u5B58\uFF08\u5F15\u64CE\u81EA\u52A8\u9075\u5B88\uFF09", "success");
        refreshConsole();
      } catch (e) {
        showToast("" + e, "error");
      }
    });
    knob("optReplyMode", "reply_mode", (el) => el.value);
    knob("optWarmupGate", "warmup_gate", (el) => el.checked ? "1" : "0");
    knob("optDryRun", "dry_run", (el) => el.checked ? "1" : "0");
    knob("optIntentMin", "intent_min", (el) => String(el.value || 40));
    knob("optQuietStart", "quiet_start", (el) => String(el.value || ""));
    knob("optQuietEnd", "quiet_end", (el) => String(el.value || ""));
  }
  function consStatusCard(icon, title, color, desc) {
    return `<div class="card" style="padding:16px;border-left:5px solid ${color};">
    <div style="font-size:18px;font-weight:700;">${icon} ${escapeHtml(title)}</div>
    <div class="text-muted" style="margin-top:6px;font-size:13px;">${escapeHtml(desc)}</div></div>`;
  }
  function consAttn(t2, d, btn, act) {
    return `<div style="display:flex;justify-content:space-between;align-items:center;margin-top:8px;gap:8px;">
    <div><div>${escapeHtml(t2)}</div><div class="text-muted" style="font-size:12px;">${escapeHtml(d)}</div></div>
    <button class="btn btn-small btn-primary" onclick="${act}">${escapeHtml(btn)}</button></div>`;
  }
  function consKpi(label, val, hl) {
    return `<div style="flex:1;min-width:72px;text-align:center;padding:8px;background:var(--bg-subtle,#f7f7f7);border-radius:8px;">
    <div style="font-size:20px;font-weight:700;${hl ? "color:#16a34a;" : ""}">${val}</div>
    <div class="text-muted" style="font-size:11px;">${label}</div></div>`;
  }
  function consSetVal(id, v) {
    const e = document.getElementById(id);
    if (e && document.activeElement !== e) e.value = v;
  }
  function consSetChk(id, v) {
    const e = document.getElementById(id);
    if (e && document.activeElement !== e) e.checked = !!v;
  }
  async function refreshConsole() {
    let st = null, ms = null, nur = [];
    try {
      st = await invoke2("get_run_state");
    } catch (_e) {
    }
    try {
      ms = await invoke2("get_marketing_stats");
    } catch (_e) {
    }
    try {
      nur = await invoke2("get_nurture_overview");
    } catch (_e) {
    }
    const apEl = document.getElementById("consAutopilot");
    if (!st || !apEl) {
      if (apEl) apEl.innerHTML = '<div class="text-muted" style="padding:12px;">\u9700\u8981\u684C\u9762\u5E94\u7528\u8FD0\u884C\u5F15\u64CE\uFF08\u8BF7\u7528 npm run tauri:dev \u542F\u52A8\uFF09</div>';
      return;
    }
    const btn = document.getElementById("btnAutopilot");
    if (st.autopilot) {
      if (btn) btn.textContent = "\u23F8 \u6682\u505C";
      apEl.innerHTML = consStatusCard(
        "\u{1F7E2}",
        "\u5168\u81EA\u52A8\u8FD0\u884C\u4E2D",
        "#16a34a",
        `\u5F15\u64CE\u81EA\u52A8\uFF1A\u517B\u53F7 \u2192 \u4F53\u68C0 \u2192 \u6309\u5173\u952E\u8BCD\u53D1\u73B0\u5E16\u5B50 \u2192 AI \u5224\u5B9A\u610F\u5411 \u2192 \u751F\u6210\u5E76\u76F4\u63A5\u53D1\u5E03\u56DE\u590D\uFF0C\u5168\u7A0B\u4E0D\u7528\u4F60\u786E\u8BA4\u3002\u5DF2\u5904\u7406 ${st.processed} \u4E2A\u4EFB\u52A1\u3002`
      );
    } else if (st.running) {
      if (btn) btn.textContent = "\u{1F7E2} \u8F6C\u5168\u81EA\u52A8";
      const why = st.dry_run ? "\u6F14\u7EC3\u4E2D\uFF08\u53EA\u8DD1\u4E0D\u53D1\uFF09" : st.reply_mode === "review" ? "\u534A\u81EA\u52A8\uFF08\u56DE\u590D\u8981\u4F60\u5BA1\u6838\u624D\u53D1\uFF09" : "\u8FD0\u884C\u4E2D";
      apEl.innerHTML = consStatusCard("\u{1F7E1}", `\u5F15\u64CE\u8FD0\u884C\u4E2D \xB7 ${why}`, "#d97706", "\u70B9\u300C\u8F6C\u5168\u81EA\u52A8\u300D= \u76F4\u63A5\u53D1\u5E03\u3001\u4E0D\u518D\u9010\u6761\u786E\u8BA4\u3002");
    } else {
      if (btn) btn.textContent = "\u{1F7E2} \u4E00\u952E\u5168\u81EA\u52A8";
      apEl.innerHTML = consStatusCard("\u23F8", "\u5DF2\u6682\u505C", "#6b7280", "\u70B9\u300C\u4E00\u952E\u5168\u81EA\u52A8\u300D\u5373\u53EF\u8BA9\u5B83\u81EA\u5DF1\u8DD1\u8D77\u6765\uFF08\u517B\u53F7+\u83B7\u5BA2\u56DE\u590D\uFF0C\u65E0\u9700\u4F60\u786E\u8BA4\uFF09\u3002");
    }
    const at = document.getElementById("consAttention");
    if (at) {
      const items = [];
      if (st.unhealthy_accounts > 0) items.push(consAttn(`\u26A0\uFE0F ${st.unhealthy_accounts} \u4E2A\u5DF2\u914D\u7F6E\u8D26\u53F7\u6389\u767B\u5F55`, "\u5728\u5BF9\u5E94\u8EAB\u4EFD\u7684\u6D4F\u89C8\u5668\u91CC\u91CD\u65B0\u767B\u5F55\u8BE5\u5E73\u53F0\u5373\u53EF", "\u53BB\u8EAB\u4EFD", `navigateTo('personas')`));
      if ((st.unconfigured_accounts || 0) > 0) items.push(consAttn(`\u{1F9E9} ${st.unconfigured_accounts} \u4E2A\u8D26\u53F7\u5F85\u914D\u7F6E`, "\u8FD8\u6CA1\u7ED1\u5B9A\u72EC\u7ACB\u8EAB\u4EFD(profile/IP)\uFF0C\u53BB\u300C\u8EAB\u4EFD\u9694\u79BB\u300D\u4E00\u952E\u914D\u7F6E\uFF08\u8FD9\u4E0D\u662F\u6545\u969C\uFF09", "\u53BB\u914D\u7F6E", `navigateTo('personas')`));
      if (st.reply_mode === "review" && st.pending_review > 0) items.push(consAttn(`\u{1F4DD} ${st.pending_review} \u6761\u56DE\u590D\u5F85\u4F60\u5BA1\u6838`, "\u60F3\u514D\u786E\u8BA4\u5C31\u628A\u4E0A\u9762\u300C\u56DE\u590D\u6A21\u5F0F\u300D\u6539\u6210\u5168\u81EA\u52A8", "\u53BB\u5BA1\u6838", `navigateTo('tasks')`));
      if (st.blocked_tasks > 0) items.push(consAttn(`\u26D4 ${st.blocked_tasks} \u4E2A\u4EFB\u52A1\u5361\u4F4F`, "\u591A\u4E3A\u8D26\u53F7\u672A\u7ED1\u5B9A profile \u6216\u517B\u53F7\u671F\u5185\uFF08\u517B\u719F\u4F1A\u81EA\u52A8\u89E3\u9501\uFF09", "\u53BB\u770B", `navigateTo('tasks')`));
      at.innerHTML = items.length ? `<div class="card" style="padding:12px;border-left:4px solid #d97706;"><strong>\u9700\u8981\u4F60\u5904\u7406</strong>${items.join("")}</div>` : `<div class="card" style="padding:12px;border-left:4px solid #16a34a;"><strong style="color:#16a34a;">\u2705 \u5168\u81EA\u52A8\u8FD0\u884C\u4E2D\uFF0C\u76EE\u524D\u65E0\u9700\u4F60\u64CD\u4F5C</strong></div>`;
    }
    consSetVal("optReplyMode", st.reply_mode);
    consSetChk("optWarmupGate", st.warmup_gate);
    consSetChk("optDryRun", st.dry_run);
    consSetVal("optIntentMin", String(st.intent_min));
    consSetVal("optQuietStart", st.quiet_start != null ? String(st.quiet_start) : "");
    consSetVal("optQuietEnd", st.quiet_end != null ? String(st.quiet_end) : "");
    const today = document.getElementById("consToday");
    if (today) {
      const t2 = ms?.totals;
      const todayNur = (nur || []).reduce((s, n) => s + (n.today_done || 0), 0);
      today.innerHTML = t2 ? `<div style="display:flex;gap:8px;flex-wrap:wrap;">${consKpi("\u517B\u53F7\u573A\u6B21", todayNur)}${consKpi("\u53D1\u73B0\u5E16\u5B50", t2.discovered)}${consKpi("\u5DF2\u56DE\u590D", t2.replied)}${consKpi("\u5F85\u5BA1", t2.pending_review)}${consKpi("\u7EBF\u7D22", t2.leads)}${consKpi("\u8F6C\u5316", t2.converted, true)}</div>` : '<span class="text-muted">\u6682\u65E0\u6570\u636E</span>';
    }
  }
  var PLATFORM_ICONS = {
    // 国际社交
    twitter: "\u{1F426}",
    reddit: "\u{1F534}",
    linkedin: "\u{1F4BC}",
    facebook: "\u{1F4D8}",
    // 开发者平台
    github: "\u{1F419}",
    devto: "\u{1F4BB}",
    producthunt: "\u{1F680}",
    hackernews: "\u{1F7E0}",
    medium: "\u{1F4DD}",
    hashnode: "#\uFE0F\u20E3",
    indiehackers: "\u{1F4A1}",
    betalist: "\u{1F3AF}",
    alternativeto: "\u{1F504}",
    // 中国平台
    zhihu: "\u77E5",
    weibo: "\u5FAE",
    v2ex: "V2",
    sspai: "\u6D3E",
    jike: "\u26A1",
    xiaohongshu: "\u{1F4D5}",
    segmentfault: "SF",
    csdn: "C",
    oschina: "\u5F00",
    // 日本平台
    qiita: "Q",
    zenn: "Z",
    note: "\u{1F4D3}",
    // 韩国平台
    naver_blog: "N",
    // 俄罗斯平台
    habr: "H",
    vk: "VK",
    // 通讯
    telegram: "\u2708\uFE0F",
    discord: "\u{1F4AC}",
    slack: "\u{1F4BC}"
  };
  var campaigns = [];
  async function loadCampaigns() {
    try {
      const data = await invoke2("list_campaigns");
      campaigns = data.campaigns || [];
      updateCampaignStats(data.stats || {});
      renderCampaigns();
    } catch (error) {
      console.error("Failed to load campaigns:", error);
      campaigns = [];
      renderCampaigns();
    }
  }
  function updateCampaignStats(stats) {
    document.getElementById("campaignStatActive").textContent = stats.active?.toString() || "0";
    document.getElementById("campaignStatScheduled").textContent = stats.scheduled?.toString() || "0";
    document.getElementById("campaignStatCompleted").textContent = stats.completed?.toString() || "0";
    document.getElementById("campaignStatTotalTasks").textContent = stats.total_tasks?.toString() || "0";
  }
  function renderCampaigns() {
    const container = document.getElementById("campaignsList");
    const empty = document.getElementById("emptyCampaigns");
    if (!container) return;
    const statusFilter = document.getElementById("campaignFilterStatus")?.value || "all";
    const platformFilter = document.getElementById("campaignFilterPlatform")?.value || "all";
    let filtered = campaigns;
    if (statusFilter !== "all") {
      filtered = filtered.filter((c) => c.status === statusFilter);
    }
    if (platformFilter !== "all") {
      filtered = filtered.filter((c) => c.platforms?.includes(platformFilter));
    }
    if (filtered.length === 0) {
      if (empty) empty.style.display = "block";
      container.innerHTML = "";
      return;
    }
    if (empty) empty.style.display = "none";
    container.innerHTML = filtered.map((c) => renderCampaignCard(c)).join("");
  }
  function renderCampaignCard(campaign) {
    const progress = campaign.total_tasks > 0 ? Math.round(campaign.completed_tasks / campaign.total_tasks * 100) : 0;
    return `
    <div class="campaign-card" data-id="${campaign.id}">
      <div class="campaign-card-header">
        <div class="campaign-info">
          <div class="campaign-name">${escapeHtml(campaign.name)}</div>
          <div class="campaign-product">${escapeHtml(campaign.product_name || "No product")}</div>
        </div>
        <span class="campaign-status ${campaign.status}">${campaign.status}</span>
      </div>
      <div class="campaign-card-body">
        <div class="campaign-platforms">
          ${(campaign.platforms || []).map((p) => `
            <span class="campaign-platform-tag">${PLATFORM_ICONS[p] || "\u{1F4C4}"} ${p}</span>
          `).join("")}
        </div>
        <div class="campaign-progress-section">
          <div class="campaign-progress-header">
            <span class="campaign-progress-label">Progress</span>
            <span class="campaign-progress-value">${campaign.completed_tasks}/${campaign.total_tasks} tasks</span>
          </div>
          <div class="campaign-progress-bar">
            <div class="campaign-progress-fill" style="width: ${progress}%"></div>
          </div>
        </div>
        <div class="campaign-stats-row">
          <span class="campaign-stat-item">\u{1F4CA} <strong>${progress}%</strong> complete</span>
          <span class="campaign-stat-item">\u{1F4C5} ${formatDate(campaign.created_at)}</span>
        </div>
      </div>
      <div class="campaign-card-footer">
        <span class="campaign-timing">
          ${campaign.started_at ? "Started " + formatDate(campaign.started_at) : "Not started"}
        </span>
        <div class="campaign-actions">
          ${campaign.status === "running" ? `
            <button class="btn btn-small btn-secondary" onclick="pauseCampaign('${campaign.id}')">Pause</button>
          ` : campaign.status === "paused" || campaign.status === "draft" ? `
            <button class="btn btn-small btn-primary" onclick="startCampaign('${campaign.id}')">Start</button>
          ` : ""}
          <button class="btn btn-small btn-secondary" onclick="viewCampaign('${campaign.id}')">Details</button>
          ${campaign.status === "draft" ? `
            <button class="btn btn-small btn-danger" onclick="deleteCampaign('${campaign.id}')">Delete</button>
          ` : ""}
        </div>
      </div>
    </div>
  `;
  }
  function formatDate(dateStr) {
    if (!dateStr) return "--";
    try {
      const date = new Date(dateStr);
      return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
    } catch {
      return "--";
    }
  }
  function showCreateCampaignModal() {
    const productSelect = document.getElementById("campaignProduct");
    if (productSelect) {
      productSelect.innerHTML = '<option value="">Select a product...</option>' + products.map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
    }
    const scheduleType = document.getElementById("campaignScheduleType");
    const timeGroup = document.getElementById("scheduleTimeGroup");
    if (scheduleType && timeGroup) {
      scheduleType.onchange = () => {
        timeGroup.style.display = scheduleType.value === "scheduled" ? "block" : "none";
      };
    }
    openModal("createCampaignModal");
  }
  window.showCreateCampaignModal = showCreateCampaignModal;
  async function createCampaign(startImmediately = false) {
    const name = document.getElementById("campaignName")?.value?.trim();
    const productId = document.getElementById("campaignProduct")?.value;
    const description = document.getElementById("campaignDescription")?.value?.trim();
    if (!name) {
      showToast("Please enter a campaign name", "error");
      return;
    }
    if (!productId) {
      showToast("Please select a product", "error");
      return;
    }
    const platformCheckboxes = document.querySelectorAll('input[name="campaignPlatform"]:checked');
    const platforms = Array.from(platformCheckboxes).map((cb) => cb.value);
    if (platforms.length === 0) {
      showToast(t("msg.selectPlatform"), "error");
      return;
    }
    const postTypeCheckboxes = document.querySelectorAll('input[name="postType"]:checked');
    const postTypes = Array.from(postTypeCheckboxes).map((cb) => cb.value);
    const langCheckboxes = document.querySelectorAll('input[name="campaignLang"]:checked');
    const languages = Array.from(langCheckboxes).map((cb) => cb.value);
    const keywordsText = document.getElementById("campaignKeywords")?.value || "";
    const keywords2 = keywordsText.split("\n").map((k) => k.trim()).filter((k) => k);
    const scheduleType = document.getElementById("campaignScheduleType")?.value || "immediate";
    const startTime = document.getElementById("campaignStartTime")?.value;
    const postsPerDay = parseInt(document.getElementById("campaignPostsPerDay")?.value) || 3;
    const duration = parseInt(document.getElementById("campaignDuration")?.value) || 7;
    try {
      await invoke2("create_campaign", {
        name,
        productId,
        description,
        platforms,
        postTypes,
        languages,
        keywords: keywords2,
        scheduleType: startImmediately ? "immediate" : scheduleType,
        startTime: scheduleType === "scheduled" ? startTime : null,
        postsPerDay,
        duration,
        startImmediately
      });
      closeModal("createCampaignModal");
      showToast(startImmediately ? "Campaign started!" : "Campaign saved as draft", "success");
      loadCampaigns();
    } catch (error) {
      console.error("Failed to create campaign:", error);
      showToast("Failed to create campaign: " + error, "error");
    }
  }
  async function startCampaign(id) {
    try {
      await invoke2("start_campaign", { id });
      showToast("Campaign started!", "success");
      loadCampaigns();
    } catch (error) {
      console.error("Failed to start campaign:", error);
      showToast("Failed to start campaign: " + error, "error");
    }
  }
  window.startCampaign = startCampaign;
  async function pauseCampaign(id) {
    try {
      await invoke2("pause_campaign", { id });
      showToast("Campaign paused", "success");
      loadCampaigns();
    } catch (error) {
      console.error("Failed to pause campaign:", error);
      showToast("Failed to pause campaign: " + error, "error");
    }
  }
  window.pauseCampaign = pauseCampaign;
  async function deleteCampaign(id) {
    if (!confirm("Are you sure you want to delete this campaign?")) return;
    try {
      await invoke2("delete_campaign", { id });
      showToast("Campaign deleted", "success");
      loadCampaigns();
    } catch (error) {
      console.error("Failed to delete campaign:", error);
      showToast("Failed to delete campaign: " + error, "error");
    }
  }
  window.deleteCampaign = deleteCampaign;
  function viewCampaign(id) {
    console.log("View campaign:", id);
    showToast("Campaign details coming soon", "info");
  }
  window.viewCampaign = viewCampaign;
  function initCampaignEvents() {
    document.getElementById("btnCreateCampaign")?.addEventListener("click", showCreateCampaignModal);
    document.getElementById("btnNewCampaign")?.addEventListener("click", showCreateCampaignModal);
    document.getElementById("btnSaveCampaignDraft")?.addEventListener("click", () => createCampaign(false));
    document.getElementById("btnStartCampaign")?.addEventListener("click", () => createCampaign(true));
    document.getElementById("campaignFilterStatus")?.addEventListener("change", renderCampaigns);
    document.getElementById("campaignFilterPlatform")?.addEventListener("change", renderCampaigns);
  }
  async function loadProducts() {
    try {
      products = await invoke2("list_products");
      renderProducts();
    } catch (error) {
      console.error("Failed to load products:", error);
    }
  }
  function renderProducts() {
    const grid = document.getElementById("productsGrid");
    const empty = document.getElementById("emptyProducts");
    const batchBar = document.getElementById("productsBatchBar");
    if (!grid || !empty) return;
    if (products.length === 0) {
      grid.style.display = "none";
      empty.style.display = "block";
      if (batchBar) batchBar.style.display = "none";
      return;
    }
    grid.style.display = "grid";
    empty.style.display = "none";
    if (batchBar) batchBar.style.display = "flex";
    grid.innerHTML = products.map((product) => {
      const isSelected = selectedProductIds.has(product.id);
      return `
    <div class="product-card ${isSelected ? "selected" : ""}" data-id="${product.id}">
      <label class="product-checkbox">
        <input type="checkbox" ${isSelected ? "checked" : ""} onchange="toggleProductSelection('${product.id}', this.checked)">
      </label>
      <div class="product-header">
        <span class="product-name">${escapeHtml(product.name)}</span>
        <span class="product-type">${product.product_type || "tool"}</span>
      </div>
      <div class="product-tagline">${escapeHtml(product.tagline || "")}</div>
      <div class="product-url">${escapeHtml(product.url)}</div>
      <div class="product-meta">
        <span>Priority: ${product.priority}</span>
        <span>Weight: ${product.weight}</span>
      </div>
      <div class="product-actions">
        <button class="btn btn-small btn-secondary" onclick="editProduct('${product.id}')">Edit</button>
        <button class="btn btn-small btn-danger" onclick="deleteProduct('${product.id}')">Delete</button>
      </div>
    </div>
  `;
    }).join("");
    updateBatchBarStatus();
  }
  window.toggleProductSelection = function(id, checked) {
    if (checked) {
      selectedProductIds.add(id);
    } else {
      selectedProductIds.delete(id);
    }
    const card = document.querySelector(`.product-card[data-id="${id}"]`);
    if (card) card.classList.toggle("selected", checked);
    updateBatchBarStatus();
  };
  function selectAllProducts() {
    products.forEach((p) => selectedProductIds.add(p.id));
    renderProducts();
  }
  function clearProductSelection() {
    selectedProductIds.clear();
    renderProducts();
  }
  function updateBatchBarStatus() {
    const countEl = document.getElementById("selectedCount");
    const publishBtn = document.getElementById("btnBatchPublish");
    const count = selectedProductIds.size;
    if (countEl) {
      countEl.textContent = count > 0 ? `${count} selected` : "No selection";
    }
    if (publishBtn) {
      publishBtn.disabled = count === 0;
      publishBtn.textContent = count > 0 ? `Publish ${count} Products` : "Publish Selected";
    }
  }
  async function batchPublishSelected() {
    if (selectedProductIds.size === 0) {
      showToast(t("msg.selectProduct"), "error");
      return;
    }
    navigateTo("publish");
  }
  async function saveProduct() {
    const name = document.getElementById("productName")?.value;
    const url = document.getElementById("productUrl")?.value;
    const tagline = document.getElementById("productTagline")?.value;
    const description = document.getElementById("productDescription")?.value;
    const productType = document.getElementById("productType")?.value;
    const priority = parseInt(document.getElementById("productPriority")?.value) || 5;
    if (!name || !url) {
      showToast("Name and URL are required", "error");
      return;
    }
    try {
      await invoke2("create_product", {
        name,
        url,
        tagline: tagline || null,
        description: description || null,
        productType: productType || "tool",
        priority,
        weight: 5
      });
      closeModal("modalAddProduct");
      await loadProducts();
      showToast(t("msg.productAdded"), "success");
    } catch (error) {
      console.error("Failed to save product:", error);
      showToast("Failed to save product", "error");
    }
  }
  async function analyzeUrl() {
    const url = document.getElementById("analyzeUrl")?.value;
    if (!url) {
      showToast("Please enter a URL", "error");
      return;
    }
    try {
      const result = await invoke2("analyze_url", { url });
      document.getElementById("productName").value = result.name || "";
      document.getElementById("productUrl").value = result.url || url;
      document.getElementById("productTagline").value = result.tagline || "";
      document.getElementById("productDescription").value = result.description || "";
      document.querySelector('.tab[data-tab="manual"]')?.dispatchEvent(new Event("click"));
      showToast("URL analyzed", "success");
    } catch (error) {
      console.error("Failed to analyze URL:", error);
      showToast("Failed to analyze URL", "error");
    }
  }
  window.deleteProduct = async function(id) {
    if (!confirm("Delete this product?")) return;
    try {
      await invoke2("delete_product", { id });
      await loadProducts();
      showToast(t("msg.productDeleted"), "success");
    } catch (error) {
      console.error("Failed to delete product:", error);
      showToast("Failed to delete product", "error");
    }
  };
  window.editProduct = function(id) {
    showToast("Edit feature coming soon", "info");
  };
  async function loadAccounts() {
    try {
      accounts = await invoke2("list_accounts");
      try {
        personasCache = await invoke2("persona_list") || [];
      } catch {
        personasCache = [];
      }
      try {
        airportStatusCache = await invoke2("airport_status");
      } catch {
        airportStatusCache = null;
      }
      await loadBrowserProfiles();
      await loadAvailableProfiles();
      await loadAccountLifecycles();
      renderAccounts();
      updateHealthOverview();
      await loadRegisterPlatforms();
      await loadGmailStatus();
    } catch (error) {
      console.error("Failed to load accounts:", error);
    }
  }
  function updateHealthOverview() {
    const totalAccounts = accounts.length;
    if (totalAccounts === 0) {
      document.getElementById("overallHealthPercent").textContent = "--%";
      document.getElementById("healthStatActive").textContent = "0";
      document.getElementById("healthStatWarning").textContent = "0";
      document.getElementById("healthStatBanned").textContent = "0";
      document.getElementById("healthStatWarmup").textContent = "0";
      return;
    }
    const activeCount = accounts.filter((a) => a.status === "active").length;
    const warningCount = accounts.filter((a) => a.status === "warning" || a.status === "limited").length;
    const bannedCount = accounts.filter((a) => a.status === "banned" || a.status === "suspended").length;
    const warmupCount = accounts.filter((a) => a.status === "warmup").length;
    const healthyCount = activeCount + warmupCount;
    const healthPercent = Math.round(healthyCount / totalAccounts * 100);
    const circle = document.querySelector(".health-circle .circle-progress");
    if (circle) {
      circle.setAttribute("stroke-dasharray", `${healthPercent}, 100`);
      circle.classList.remove("warning", "error");
      if (healthPercent < 50) circle.classList.add("error");
      else if (healthPercent < 80) circle.classList.add("warning");
    }
    document.getElementById("overallHealthPercent").textContent = healthPercent + "%";
    document.getElementById("healthStatActive").textContent = activeCount.toString();
    document.getElementById("healthStatWarning").textContent = warningCount.toString();
    document.getElementById("healthStatBanned").textContent = bannedCount.toString();
    document.getElementById("healthStatWarmup").textContent = warmupCount.toString();
  }
  function getHealthBadge(account) {
    const status = account.status || "active";
    const healthScore = account.health_score || 100;
    const warmupStage = account.warmup_stage || "none";
    if (status === "banned" || status === "suspended") {
      return '<span class="account-health-badge danger">\u{1F534} Banned</span>';
    }
    if (status === "warning" || status === "limited" || healthScore < 50) {
      return '<span class="account-health-badge warning">\u26A0\uFE0F At Risk</span>';
    }
    if (warmupStage !== "none" && warmupStage !== "complete") {
      const warmupPercent = warmupStage === "day1" ? 20 : warmupStage === "day3" ? 40 : warmupStage === "day7" ? 70 : 90;
      return `<span class="account-health-badge warmup">\u{1F525} Warming ${warmupPercent}%</span>`;
    }
    return '<span class="account-health-badge healthy">\u2705 Healthy</span>';
  }
  var availableProfiles = [];
  async function loadAvailableProfiles() {
    try {
      availableProfiles = await invoke2("get_available_browser_profiles");
    } catch (error) {
      console.error("Failed to load available profiles:", error);
      availableProfiles = [];
    }
  }
  var accountLifecycles = /* @__PURE__ */ new Map();
  async function loadAccountLifecycles() {
    for (const account of accounts) {
      try {
        const lifecycle = await invoke2("get_account_lifecycle", { accountId: account.id });
        accountLifecycles.set(account.id, lifecycle);
      } catch (e) {
        console.error("Failed to load lifecycle for", account.id, e);
      }
    }
  }
  var personasCache = [];
  var airportStatusCache = null;
  var selectedPersonaId = null;
  function renderAccounts() {
    const list = document.getElementById("accountsList");
    const empty = document.getElementById("emptyAccounts");
    if (!list || !empty) return;
    const a = airportStatusCache;
    const airportBar = `<div class="card" style="margin:0 0 10px;padding:10px 14px;display:flex;align-items:center;gap:10px;flex-wrap:wrap;border-left:4px solid ${a && a.configured ? "#1a9d4a" : "#d97706"};">
    <span style="font-weight:700;">\u{1F310} \u673A\u573A\u4EE3\u7406</span>
    <span class="text-muted" style="font-size:12px;">${a && a.configured ? `\u8282\u70B9\u6C60 ${a.total} \u4E2A\uFF08\u7A7A\u95F2 ${a.free}\uFF09\xB7 \u6BCF\u4E2A\u90AE\u7BB1\u5206\u4E00\u4E2A\u72EC\u7ACB\u51FA\u53E3 IP` : "\u672A\u914D\u7F6E\u2014\u2014\u914D\u4E86\u624D\u80FD\u7ED9\u90AE\u7BB1\u5206\u914D\u72EC\u7ACB IP"}</span>
    <button class="btn btn-small btn-secondary" style="margin-left:auto;" onclick="setAirportPrompt()">${a && a.configured ? "\u6362/\u5237\u65B0\u8BA2\u9605" : "\u8BBE\u7F6E\u673A\u573A\u8BA2\u9605"}</button>
  </div>`;
    const groups = /* @__PURE__ */ new Map();
    for (const acc of accounts) {
      const key = acc.persona_id || "__none__";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(acc);
    }
    const hasUnassigned = groups.has("__none__");
    const tabIds = [...personasCache.map((p) => p.id), ...hasUnassigned ? ["__none__"] : []];
    list.style.display = "block";
    empty.style.display = "none";
    if (tabIds.length === 0) {
      list.innerHTML = airportBar + `<div class="card" style="padding:18px;text-align:center;">
      <div class="text-muted">\u8FD8\u6CA1\u6709\u90AE\u7BB1\u3002\u7528\u4E00\u4E2A\u771F\u5B9E Gmail \u65B0\u5EFA\u7B2C\u4E00\u4E2A \u2192</div>
      <button class="btn btn-primary" style="margin-top:10px;" onclick="createPersonaPrompt()">+ \u65B0\u5EFA Gmail</button></div>`;
      return;
    }
    if (!selectedPersonaId || !tabIds.includes(selectedPersonaId)) selectedPersonaId = tabIds[0];
    const tabs = tabIds.map((id) => {
      const active = id === selectedPersonaId;
      let label;
      if (id === "__none__") label = `\u{1F9E9} \u672A\u5F52\u5C5E(${groups.get("__none__").length})`;
      else {
        const p = personasCache.find((x) => x.id === id);
        label = `\u{1F4E7} ${p?.email || "\u90AE\u7BB1"}`;
      }
      return `<button class="btn btn-small ${active ? "btn-primary" : "btn-secondary"}" onclick="selectEmail('${id}')">${escapeHtml(label)}</button>`;
    }).join("");
    const tabBar = `<div class="card" style="margin:0 0 12px;padding:10px 14px;display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
    <span style="font-weight:700;">\u{1F4E7} \u90AE\u7BB1\uFF1A</span>${tabs}
    <button class="btn btn-small btn-primary" style="margin-left:auto;" onclick="createPersonaPrompt()" title="\u7528\u4E00\u4E2A\u771F\u5B9E Gmail \u65B0\u5EFA\u4E00\u5957\u72EC\u7ACB\u8EAB\u4EFD">+ \u65B0\u5EFA Gmail</button>
  </div>`;
    const sel = selectedPersonaId;
    const accts = groups.get(sel) || [];
    let panel = "";
    if (sel === "__none__") {
      panel = `<div class="card" style="margin:0 0 8px;padding:10px 14px;border-left:4px solid #d97706;">
      <div style="font-weight:600;">\u{1F9E9} \u672A\u5F52\u5C5E\u90AE\u7BB1 \xB7 ${accts.length} \u4E2A\u8D26\u53F7</div>
      <div class="text-muted" style="font-size:12px;">\u8FD9\u4E9B\u8D26\u53F7\u8FD8\u6CA1\u6302\u5230\u67D0\u4E2A Gmail \u4E0B\u3002\u5728\u8D26\u53F7\u4E0A\u9009\u300C\u5F52\u5C5E\u8EAB\u4EFD\u300D\u5F52\u7C7B\u5373\u53EF\u3002</div></div>`;
      panel += accts.map((x) => renderAccountCard(x)).join("");
    } else {
      const p = personasCache.find((x) => x.id === sel);
      panel = `<div class="card" style="margin:0 0 8px;padding:12px 14px;border-left:4px solid #4a8cff;">
      <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;">
        <span style="font-weight:700;font-size:15px;">\u{1F4E7} ${escapeHtml(p?.email || "\u90AE\u7BB1")}</span>
        <span class="text-muted" style="font-size:12px;">${p?.region ? escapeHtml(p.region) : "\u{1F310} \u8282\u70B9\u672A\u5206\u914D"}${p?.profile_id ? " \xB7 \u72EC\u7ACB\u6D4F\u89C8\u5668 " + escapeHtml(p.profile_id) : ""} \xB7 ${accts.length} \u4E2A\u8D26\u53F7</span>
      </div>
      <div style="display:flex;gap:6px;flex-wrap:wrap;margin-top:10px;">
        <button class="btn btn-small btn-secondary" onclick="personaGmailLogin('${sel}')" title="\u6253\u5F00\u8FD9\u4E2A\u90AE\u7BB1\u7684\u6D4F\u89C8\u5668\u767B\u5F55\u5B83\u7684 Gmail\uFF08\u57FA\u7840\u767B\u5F55\uFF0C\u5148\u505A\u8FD9\u6B65\uFF09">\u{1F4E7} \u767B\u5F55Gmail</button>
        <button class="btn btn-small btn-success" onclick="personaProvisionAll('${sel}','${escapeHtml(p?.email || "")}')" title="\u9010\u5E73\u53F0\uFF1A\u6709\u5C31\u767B\u5F55\u3001\u6CA1\u6709\u5C31\u6CE8\u518C">\u{1F680} \u68C0\u67E5\u5E76\u5F00\u901A\u8D26\u53F7</button>
        <button class="btn btn-small btn-primary" onclick="window.__addAccountPersona='${sel}';openModal('modalAddAccount');">+ \u52A0\u8D26\u53F7</button>
        <button class="btn btn-small btn-secondary" style="margin-left:auto;color:#e55;" onclick="deletePersonaAcct('${sel}','${escapeHtml(p?.email || "")}')" title="\u5220\u9664\u8FD9\u4E2A\u90AE\u7BB1\u8EAB\u4EFD">\u5220\u9664\u6B64\u90AE\u7BB1</button>
      </div></div>`;
      panel += accts.length ? accts.map((x) => renderAccountCard(x)).join("") : `<div class="card" style="padding:16px;text-align:center;"><div class="text-muted">\u8FD9\u4E2A\u90AE\u7BB1\u8FD8\u6CA1\u6709\u8D26\u53F7 \u2014\u2014 \u70B9\u4E0A\u9762\u300C\u{1F680} \u68C0\u67E5\u5E76\u5F00\u901A\u8D26\u53F7\u300D\u81EA\u52A8\u5F00\u901A\u5404\u5E73\u53F0\uFF0C\u6216\u300C+ \u52A0\u8D26\u53F7\u300D\u624B\u52A8\u52A0\u3002</div></div>`;
    }
    list.innerHTML = airportBar + tabBar + panel;
  }
  window.selectEmail = function(id) {
    selectedPersonaId = id;
    renderAccounts();
  };
  window.setAirportPrompt = async function() {
    const url = (prompt("\u7C98\u8D34\u4F60\u7684\u673A\u573A\u8BA2\u9605\u94FE\u63A5\uFF08Clash \u8BA2\u9605\uFF09\uFF1A") || "").trim();
    if (!url) return;
    showToast("\u6B63\u5728\u62C9\u53D6\u8282\u70B9\u2026", "info");
    try {
      const msg = await invoke2("airport_set_subscription", { url });
      showToast("" + msg, "success");
      await loadAccounts();
    } catch (e) {
      showToast("\u8BA2\u9605\u5931\u8D25\uFF1A" + e, "error");
    }
  };
  window.createPersonaPrompt = async function() {
    const email = (prompt("\u8F93\u5165\u4E00\u4E2A\u771F\u5B9E Gmail\uFF08\u8FD9\u4E2A\u90AE\u7BB1\u4F1A\u6210\u4E3A\u4E00\u5957\u72EC\u7ACB\u8EAB\u4EFD\uFF1A\u72EC\u7ACB\u6D4F\u89C8\u5668+IP+\u6307\u7EB9\uFF09\uFF1A") || "").trim();
    if (!email) return;
    if (!email.includes("@")) {
      showToast("\u8BF7\u8F93\u5165\u6709\u6548\u7684 Gmail \u5730\u5740", "error");
      return;
    }
    showToast("\u6B63\u5728\u521B\u5EFA\u8EAB\u4EFD\u2026\uFF08\u5EFA\u6D4F\u89C8\u5668+\u968F\u673A\u6307\u7EB9+\u5206\u914D\u51FA\u53E3\u8282\u70B9\uFF0C\u7EA6 5-10 \u79D2\uFF09", "info");
    try {
      const dto = await invoke2("persona_create", { email });
      if (dto && dto.id) selectedPersonaId = dto.id;
      showToast(`\u90AE\u7BB1\u5DF2\u5EFA\u597D \u2713 \u5DF2\u6253\u5F00 Google \u767B\u5F55\u9875 \u2192 \u8BF7\u5728\u5F39\u51FA\u7684\u6D4F\u89C8\u5668\u7A97\u53E3\u767B\u5F55 ${email}\uFF08\u57FA\u7840\u767B\u5F55\uFF0C\u53EA\u9700\u4E00\u6B21\uFF1B\u767B\u597D\u540E\u624D\u80FD\u81EA\u52A8\u6CE8\u518C/\u767B\u5F55\u8D26\u53F7\uFF09`, "info");
      await loadAccounts();
    } catch (e) {
      showToast("\u521B\u5EFA\u5931\u8D25\uFF1A" + e, "error");
    }
  };
  window.personaProvisionAll = async function(id, email) {
    if (!confirm(`\u7528 ${email} \u68C0\u67E5\u5E76\u5F00\u901A\u5404\u5E73\u53F0\u8D26\u53F7\uFF1F
\u4F1A\u9010\u4E2A\u5E73\u53F0\uFF1A\u6709\u5C31\u767B\u5F55\u3001\u6CA1\u6709\u5C31\u6CE8\u518C\uFF08\u53CB\u597D\u5E73\u53F0\u5168\u81EA\u52A8\uFF1BX/Reddit \u4F1A\u6253\u5F00\u767B\u5F55\u9875\u8BA9\u4F60\u70B9\u4E00\u4E0B\uFF09\u3002
\u524D\u63D0\uFF1A\u8FD9\u4E2A\u90AE\u7BB1\u7684 Gmail \u5DF2\u5728\u5B83\u7684\u6D4F\u89C8\u5668\u91CC\u767B\u5F55\u3002`)) return;
    showToast(`\u6B63\u5728\u7528 ${email} \u5F00\u901A\u5404\u5E73\u53F0\u8D26\u53F7\u2026\uFF08\u9010\u4E2A\u5E73\u53F0\u8DD1\uFF0C\u53EF\u80FD\u8981\u51E0\u5206\u949F\uFF0C\u8BF7\u8010\u5FC3\u7B49\uFF09`, "info");
    try {
      const msg = await invoke2("persona_provision_all", { personaId: id });
      showToast("" + msg, "success");
      await loadAccounts();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.personaGmailLogin = async function(id) {
    try {
      const msg = await invoke2("persona_open_gmail_login", { personaId: id });
      showToast("" + msg, "info");
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.deletePersonaAcct = async function(id, email) {
    if (!confirm(`\u5220\u9664\u8EAB\u4EFD ${email}\uFF1F
\u4F1A\u5220\u6389\u5B83\u7684\u72EC\u7ACB\u6D4F\u89C8\u5668\u5E76\u91CA\u653E\u51FA\u53E3\u8282\u70B9\uFF1B\u540D\u4E0B\u8D26\u53F7\u4F1A\u53D8\u6210\u300C\u672A\u5F52\u5C5E\u300D\u3002`)) return;
    try {
      await invoke2("persona_delete", { id });
      showToast("\u8EAB\u4EFD\u5DF2\u5220\u9664", "success");
      await loadAccounts();
    } catch (e) {
      showToast("\u5220\u9664\u5931\u8D25\uFF1A" + e, "error");
    }
  };
  function personaSelectOptions(selectedId) {
    const opts = personasCache.map((p) => `<option value="${p.id}" ${p.id === selectedId ? "selected" : ""}>${escapeHtml(p.email)}</option>`).join("");
    return `<option value="">\u672A\u5F52\u5C5E\uFF08\u7528\u5168\u5C40 Profile\uFF09</option>${opts}`;
  }
  function renderAccountCard(account) {
    {
      const getProfileForAccount = (accountId) => browserProfiles.find((p) => p.account_id === accountId);
      const profile = getProfileForAccount(account.id);
      const hasProfile = !!profile;
      const stealthBadge = profile?.stealth_enabled ? '<span class="badge badge-stealth" title="Stealth Mode">\u{1F6E1}\uFE0F</span>' : "";
      const fingerprintBadge = profile?.fingerprint_id ? '<span class="badge badge-fingerprint" title="Fingerprint Randomized">\u{1F3AD}</span>' : "";
      const proxyBadge = profile?.proxy ? `<span class="badge badge-proxy" title="${escapeHtml(profile.proxy)}">\u{1F310}</span>` : "";
      const healthBadge = getHealthBadge(account);
      const lifecycle = accountLifecycles.get(account.id);
      const stage = lifecycle?.stage || "new";
      const daysRemaining = lifecycle?.days_remaining || 0;
      const progressPercent = lifecycle?.progress_percent || 0;
      const todaySessions = lifecycle?.today?.sessions_completed || 0;
      const todayTarget = lifecycle?.today?.sessions_min || 2;
      const todayCompleted = todaySessions >= todayTarget;
      const stageBadges = {
        "new": '<span class="badge badge-new" style="background:#6c757d;color:white;">\u{1F195} \u65B0\u8D26\u53F7</span>',
        "warming": `<span class="badge badge-warming" style="background:#ffc107;color:black;">\u{1F525} \u517B\u53F7\u4E2D (${daysRemaining}\u5929)</span>`,
        "active": '<span class="badge badge-active" style="background:#28a745;color:white;">\u2705 \u6B63\u5E38</span>'
      };
      const stageBadge = stageBadges[stage] || stageBadges["new"];
      const todayProgress = lifecycle ? `
      <div class="nurture-today" style="margin: 8px 0; padding: 8px; background: var(--bg-secondary); border-radius: 6px;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 5px;">
          <span style="font-size: 12px; color: var(--text-muted);">\u4ECA\u65E5\u517B\u53F7\u8FDB\u5EA6</span>
          <span style="font-size: 12px; font-weight: bold; color: ${todayCompleted ? "var(--success)" : "var(--warning)"};">
            ${todaySessions} / ${todayTarget} \u6B21
          </span>
        </div>
        <div style="background: var(--border); border-radius: 4px; height: 6px; overflow: hidden;">
          <div style="background: ${todayCompleted ? "var(--success)" : "var(--primary)"}; height: 100%; width: ${Math.min(100, todaySessions / todayTarget * 100)}%; transition: width 0.3s;"></div>
        </div>
        ${stage === "warming" ? `
          <div style="margin-top: 5px; font-size: 11px; color: var(--text-muted);">
            \u517B\u53F7\u8FDB\u5EA6: ${progressPercent}% (\u5269\u4F59 ${daysRemaining} \u5929)
          </div>
        ` : ""}
      </div>
    ` : "";
      const profileOptions = availableProfiles.map(
        (p) => `<option value="${escapeHtml(p.id)}" ${account.profile_id === p.id ? "selected" : ""}>${escapeHtml(p.name)} (${escapeHtml(p.id)})</option>`
      ).join("");
      const personaBadge = account.persona_email ? `<span class="badge badge-profile" title="\u8EAB\u4EFD: ${escapeHtml(account.persona_email)}\uFF08\u5171\u7528\u5176\u6D4F\u89C8\u5668+IP\uFF09">\u{1F9D1}\u200D\u{1F91D}\u200D\u{1F9D1} ${escapeHtml(account.persona_email)}</span>` : '<span class="badge badge-no-profile" title="\u672A\u5F52\u5C5E\u8EAB\u4EFD">\u{1F9E9} \u672A\u5F52\u5C5E</span>';
      return `
      <div class="account-item ${hasProfile ? "has-profile" : ""}">
        <div class="account-info">
          <span class="account-platform">${escapeHtml(account.platform)}</span>
          <span class="account-username">${escapeHtml(account.username || account.email || "N/A")}</span>
          ${stageBadge}
          ${healthBadge}
          ${personaBadge}
          <span class="account-badges">${stealthBadge}${fingerprintBadge}${proxyBadge}</span>
        </div>
        ${todayProgress}
        <div class="account-profile-binding" style="margin: 8px 0;">
          <label class="text-muted" style="font-size:12px;">\u5F52\u5C5E\u8EAB\u4EFD\uFF1A</label>
          <select class="select select-small" onchange="setAccountPersona('${account.id}', this.value)" style="width: auto; min-width: 220px;">
            ${personaSelectOptions(account.persona_id)}
          </select>
        </div>
        <div class="account-nurture-stats" style="margin: 5px 0; font-size: 12px; color: var(--text-muted);">
          ${account.total_nurture_seconds > 0 ? `\u{1F331} \u7D2F\u8BA1: ${formatNurtureTime(account.total_nurture_seconds)}` : ""}
          ${account.last_nurture_at ? ` \u2022 ${t("nurture.lastNurture")}: ${formatTimeAgo(account.last_nurture_at)}` : ""}
        </div>
        <div class="account-actions">
          <button class="btn btn-small btn-primary" onclick="autoLoginAccount('${account.id}','${escapeHtml(account.platform)}')" title="\u81EA\u52A8\u767B\u5F55\uFF1A\u67E5\u767B\u5F55\u2192Google\u767B\u5F55\u2192\u5426\u5219\u6CE8\u518C">\u{1F511} \u81EA\u52A8\u767B\u5F55</button>
          <button class="btn btn-small btn-success" data-nurture-account="${account.id}" onclick="openNurtureModal('${account.id}', '${escapeHtml(account.platform)}', '${escapeHtml(account.username || account.email || "N/A")}')" title="${t("nurture.quickNurture")}">\u{1F331} ${t("nurture.quickNurture")}</button>
          ${stage === "new" ? `<button class="btn btn-small btn-warning" onclick="startWarmup('${account.id}')" title="\u5F00\u59CB\u517B\u53F7">\u{1F525} \u5F00\u59CB\u517B\u53F7</button>` : ""}
          ${!hasProfile ? `<button class="btn btn-small btn-secondary" onclick="createProfileForAccount('${account.id}', '${escapeHtml(account.platform)}')">${t("accounts.createProfile")}</button>` : ""}
          ${hasProfile ? `<button class="btn btn-small btn-secondary" onclick="toggleStealth('${profile.id}', ${!profile.stealth_enabled})" title="${profile.stealth_enabled ? "Disable" : "Enable"} Stealth">${profile.stealth_enabled ? "\u{1F6E1}\uFE0F" : "\u26A1"}</button>` : ""}
          ${hasProfile ? `<button class="btn btn-small btn-secondary" onclick="randomizeFingerprint('${profile.id}')" title="Randomize Fingerprint">\u{1F3AD}</button>` : ""}
          ${hasProfile ? `<button class="btn btn-small btn-secondary" onclick="showProxyModal('${profile.id}')" title="Set Proxy">\u{1F310}</button>` : ""}
          <button class="btn btn-small btn-danger" onclick="deleteAccount('${account.id}')">${t("accounts.delete")}</button>
        </div>
      </div>
    `;
    }
  }
  window.autoLoginAccount = async function(accountId, platform) {
    showToast(`\u6B63\u5728\u5904\u7406 ${platform}\u2026\uFF08\u67E5\u767B\u5F55\u2192\u81EA\u52A8\u767B\u5F55\uFF0C\u53EF\u80FD\u9700\u8981\u51E0\u5341\u79D2\uFF09`, "info");
    try {
      const msg = await invoke2("account_auto_login", { accountId });
      if (("" + msg).startsWith("MANUAL::")) {
        showToast(("" + msg).replace("MANUAL::", ""), "info");
      } else {
        showToast("" + msg, "success");
      }
      await loadAccounts();
    } catch (error) {
      showToast("" + error, "error");
    }
  };
  window.personaLoginAll = async function(personaId) {
    if (!confirm("\u81EA\u52A8\u767B\u5F55\u8FD9\u4E2A\u8EAB\u4EFD\u4E0B\u7684\u6240\u6709\u8D26\u53F7\uFF1F\n\u4F1A\u9010\u4E2A\u5C1D\u8BD5\uFF1A\u67E5\u767B\u5F55\u2192Google\u767B\u5F55\u2192\u5426\u5219\u6CE8\u518C\u3002\u9047\u5230\u9700\u624B\u673A/\u9A8C\u8BC1\u7801\u7684\u4F1A\u505C\u4E0B\u63D0\u793A\u4F60\u3002")) return;
    showToast("\u5F00\u59CB\u6279\u91CF\u81EA\u52A8\u767B\u5F55\u2026\uFF08\u6BCF\u4E2A\u8D26\u53F7\u51E0\u5341\u79D2\uFF0C\u8BF7\u8010\u5FC3\u7B49\uFF09", "info");
    try {
      const msg = await invoke2("persona_login_all", { personaId });
      showToast("" + msg, "success");
      await loadAccounts();
    } catch (error) {
      showToast("" + error, "error");
    }
  };
  window.setAccountPersona = async function(accountId, personaId) {
    try {
      await invoke2("set_account_persona", { accountId, personaId: personaId || null });
      showToast(personaId ? "\u5DF2\u5F52\u5C5E\u5230\u8BE5\u8EAB\u4EFD\uFF08\u81EA\u52A8\u5171\u7528\u5176\u6D4F\u89C8\u5668+IP\uFF09" : "\u5DF2\u89E3\u9664\u5F52\u5C5E", "success");
      await loadAccounts();
    } catch (error) {
      showToast("" + error, "error");
    }
  };
  window.startWarmup = async function(accountId) {
    try {
      await invoke2("start_account_nurture", { accountId });
      showToast(currentLanguage === "zh" ? "\u5DF2\u5F00\u59CB\u517B\u53F7" : "Warmup started", "success");
      await loadAccounts();
    } catch (error) {
      showToast(`Error: ${error}`, "error");
    }
  };
  async function loadRegisterPlatforms() {
    try {
      const platforms = await invoke2("get_register_platforms");
      const container = document.getElementById("registerPlatforms");
      if (!container) return;
      let html = "";
      for (const [category, platformList] of Object.entries(platforms)) {
        html += `<div class="platform-category"><h4>${category}</h4>`;
        for (const p of platformList) {
          const icons = `${p.phone ? "\u{1F4F1}" : ""} ${p.google_oauth ? "\u{1F517}" : ""}`;
          html += `<label class="checkbox"><input type="checkbox" value="${p.id}"> ${p.name} ${icons}</label>`;
        }
        html += "</div>";
      }
      container.innerHTML = html;
    } catch (error) {
      console.error("Failed to load platforms:", error);
    }
  }
  async function loadGmailStatus() {
    try {
      const status = await invoke2("get_gmail_status");
      const badge = document.getElementById("gmailBadge");
      const email = document.getElementById("gmailEmail");
      if (badge && email) {
        if (status.connected) {
          badge.textContent = "Connected";
          badge.className = "status-badge connected";
          email.textContent = status.email || "";
        } else {
          badge.textContent = "Not Connected";
          badge.className = "status-badge disconnected";
          email.textContent = "";
        }
      }
      updateRegisterButton();
    } catch (error) {
      console.error("Gmail status error:", error);
    }
  }
  async function setupGmail() {
    try {
      console.log("Setting up Gmail...");
      const result = await invoke2("setup_gmail");
      console.log("Gmail setup result:", result);
      await loadGmailStatus();
      showToast("Gmail connected successfully!", "success");
    } catch (error) {
      console.error("Gmail setup error:", error);
      showToast(error.toString(), "error");
    }
  }
  function selectAllPlatforms() {
    document.querySelectorAll('#registerPlatforms input[type="checkbox"]').forEach((cb) => {
      cb.checked = true;
    });
    updateRegisterButton();
  }
  function deselectAllPlatforms() {
    document.querySelectorAll('#registerPlatforms input[type="checkbox"]').forEach((cb) => {
      cb.checked = false;
    });
    updateRegisterButton();
  }
  function selectAllPublishPlatforms() {
    document.querySelectorAll('#platformsGroup input[type="checkbox"]').forEach((cb) => {
      cb.checked = true;
    });
  }
  function deselectAllPublishPlatforms() {
    document.querySelectorAll('#platformsGroup input[type="checkbox"]').forEach((cb) => {
      cb.checked = false;
    });
  }
  function selectAllCampaignPlatforms() {
    document.querySelectorAll('input[name="campaignPlatform"]').forEach((cb) => {
      cb.checked = true;
    });
  }
  function deselectAllCampaignPlatforms() {
    document.querySelectorAll('input[name="campaignPlatform"]').forEach((cb) => {
      cb.checked = false;
    });
  }
  function updateRegisterButton() {
    const btn = document.getElementById("btnAutoRegister");
    if (!btn) return;
    const checked = document.querySelectorAll("#registerPlatforms input:checked").length;
    btn.disabled = checked === 0;
    btn.textContent = checked > 0 ? `Auto-Login/Register (${checked})` : "Auto-Login/Register Selected";
  }
  async function autoRegister() {
    const selected = Array.from(document.querySelectorAll("#registerPlatforms input:checked")).map((cb) => cb.value);
    if (selected.length === 0) {
      showToast(t("msg.selectPlatform"), "error");
      return;
    }
    const progress = document.getElementById("registerProgress");
    if (progress) {
      progress.style.display = "block";
      progress.innerHTML = "<p>Starting auto-login/register...</p>";
    }
    for (const platform of selected) {
      try {
        if (progress) progress.innerHTML += `<p>\u23F3 ${platform}...</p>`;
        const result = await invoke2("register_platform", { platform });
        if (result.success) {
          if (progress) progress.innerHTML += `<p class="success">\u2713 ${platform}: logged in${result.username ? ` as ${result.username}` : ""}</p>`;
        } else if (result.needs_manual_verification) {
          if (progress) progress.innerHTML += `<p class="warning">\u26A0 ${platform}: ${result.verification_reason || result.error}</p>`;
        } else {
          if (progress) progress.innerHTML += `<p class="error">\u2717 ${platform}: ${result.error}</p>`;
        }
      } catch (error) {
        if (progress) progress.innerHTML += `<p class="error">\u2717 ${platform}: ${error}</p>`;
      }
    }
    await loadAccounts();
    showToast("Auto-login/register completed", "success");
  }
  async function syncAllPlatforms() {
    const progress = document.getElementById("registerProgress");
    if (progress) {
      progress.style.display = "block";
      progress.innerHTML = "<p>\u{1F504} Syncing all platforms...</p>";
    }
    try {
      const results = await invoke2("sync_all_platforms");
      let loggedIn = 0;
      let notLoggedIn = 0;
      for (const result of results) {
        if (result.success) {
          loggedIn++;
          if (progress) progress.innerHTML += `<p class="success">\u2713 ${result.platform}: logged in</p>`;
        } else {
          notLoggedIn++;
          if (progress) progress.innerHTML += `<p class="muted">\u25CB ${result.platform}: not logged in</p>`;
        }
      }
      if (progress) progress.innerHTML += `<p><strong>Summary: ${loggedIn} logged in, ${notLoggedIn} not logged in</strong></p>`;
      await loadAccounts();
      showToast(`Synced: ${loggedIn} accounts found`, "success");
    } catch (error) {
      console.error("Sync error:", error);
      if (progress) progress.innerHTML += `<p class="error">Sync failed: ${error}</p>`;
      showToast("Failed to sync platforms", "error");
    }
  }
  async function saveAccount() {
    const platform = document.getElementById("accountPlatform")?.value;
    const username = document.getElementById("accountUsername")?.value;
    const password = document.getElementById("accountPassword")?.value;
    if (!platform || !username) {
      showToast("Platform and username are required", "error");
      return;
    }
    try {
      const created = await invoke2("add_account", { platform, username, password: password || "" });
      const personaId = window.__addAccountPersona;
      if (personaId && created?.id) {
        try {
          await invoke2("set_account_persona", { accountId: created.id, personaId });
        } catch {
        }
      }
      window.__addAccountPersona = void 0;
      closeModal("modalAddAccount");
      await loadAccounts();
      showToast(t("msg.accountAdded"), "success");
    } catch (error) {
      console.error("Failed to save account:", error);
      showToast("Failed to save account", "error");
    }
  }
  window.deleteAccount = async function(id) {
    if (!confirm("Delete this account?")) return;
    try {
      await invoke2("delete_account", { id });
      await loadAccounts();
      showToast(t("msg.accountDeleted"), "success");
    } catch (error) {
      showToast("Failed to delete account", "error");
    }
  };
  async function loadBrowserProfiles() {
    try {
      browserProfiles = await invoke2("unzoo_list_profiles");
    } catch (error) {
      console.error("Failed to load browser profiles:", error);
      browserProfiles = [];
    }
  }
  window.bindProfileToAccount = async function(accountId, profileId) {
    try {
      if (profileId) {
        await invoke2("bind_account_profile", { accountId, profileId });
        showToast(`\u5DF2\u7ED1\u5B9A Profile: ${profileId}`, "success");
      } else {
        await invoke2("unbind_account_profile", { accountId });
        showToast(t("msg.profileUnbound"), "success");
      }
      await loadAccounts();
    } catch (error) {
      console.error("Failed to bind profile:", error);
      showToast(t("msg.bindFailed"), "error");
    }
  };
  window.createProfileForAccount = async function(accountId, platform) {
    try {
      const profileName = `${platform}_${accountId.substring(0, 8)}`;
      await invoke2("unzoo_create_profile", {
        name: profileName,
        platform,
        accountId
      });
      showToast("Profile created", "success");
      await loadBrowserProfiles();
      renderAccounts();
    } catch (error) {
      console.error("Failed to create profile:", error);
      showToast("Failed to create profile", "error");
    }
  };
  window.toggleStealth = async function(profileId, enabled) {
    try {
      await invoke2("unzoo_set_stealth_mode", { profileId, enabled });
      showToast(enabled ? "Stealth mode enabled" : "Stealth mode disabled", "success");
      await loadBrowserProfiles();
      renderAccounts();
    } catch (error) {
      console.error("Failed to toggle stealth:", error);
      showToast("Failed to toggle stealth mode", "error");
    }
  };
  window.randomizeFingerprint = async function(profileId) {
    try {
      await invoke2("unzoo_randomize_fingerprint", { profileId });
      showToast("Fingerprint randomized", "success");
      await loadBrowserProfiles();
      renderAccounts();
    } catch (error) {
      console.error("Failed to randomize fingerprint:", error);
      showToast("Failed to randomize fingerprint", "error");
    }
  };
  window.showProxyModal = function(profileId) {
    const profile = browserProfiles.find((p) => p.id === profileId);
    const modal = document.getElementById("modalSetProxy");
    if (modal) {
      document.getElementById("proxyProfileId").value = profileId;
      document.getElementById("proxyUrl").value = profile?.proxy || "";
      modal.classList.add("show");
    }
  };
  async function saveProxy() {
    const profileId = document.getElementById("proxyProfileId")?.value;
    const proxy = document.getElementById("proxyUrl")?.value || null;
    try {
      await invoke2("unzoo_update_profile_proxy", { profileId, proxy });
      closeModal("modalSetProxy");
      showToast("Proxy updated", "success");
      await loadBrowserProfiles();
      renderAccounts();
    } catch (error) {
      console.error("Failed to update proxy:", error);
      showToast("Failed to update proxy", "error");
    }
  }
  var nurtureInProgress = null;
  window.showNurtureModal = function(accountId, platform, username = "") {
    window.openNurtureModal(accountId, platform, username);
  };
  window.openNurtureModal = function(accountId, platform, username = "") {
    const modal = document.getElementById("modalNurture");
    if (modal) {
      resetNurtureModal();
      document.getElementById("nurtureAccountId").value = accountId;
      document.getElementById("nurturePlatform").value = platform;
      document.getElementById("nurtureAccountInfo").textContent = `${platform} - ${username || t("msg.account")}`;
      modal.classList.add("show");
    }
  };
  window.quickNurtureAccount = async function(accountId, platform, seconds = 60) {
    if (nurtureInProgress) {
      showToast(t("nurture.running"), "warning");
      return;
    }
    nurtureInProgress = accountId;
    const btn = document.querySelector(`[data-nurture-account="${accountId}"]`);
    if (btn) {
      btn.disabled = true;
      btn.innerHTML = `<span class="spinner-small"></span> ${t("nurture.running")}`;
    }
    try {
      showToast(`${t("nurture.startNurture")} (${seconds}${t("nurture.seconds")})`, "info");
      const result = await invoke2("quick_nurture", {
        accountId,
        seconds
      });
      showToast(`${t("nurture.completed")}: ${result}`, "success");
      await loadAccounts();
    } catch (error) {
      console.error("Nurture failed:", error);
      showToast(`${t("nurture.failed")}: ${error}`, "error");
    } finally {
      nurtureInProgress = null;
      if (btn) {
        btn.disabled = false;
        btn.innerHTML = `\u{1F331} ${t("nurture.quickNurture")}`;
      }
    }
  };
  var nurtureTimerInterval = null;
  var nurtureStartTime = 0;
  var nurtureTotalSeconds = 0;
  var nurtureAborted = false;
  var currentNurtureTaskId = null;
  window.startNurtureFromModal = async function() {
    const accountId = document.getElementById("nurtureAccountId")?.value;
    const platform = document.getElementById("nurturePlatform")?.value;
    const accountInfo = document.getElementById("nurtureAccountInfo")?.textContent || "";
    const seconds = parseInt(document.getElementById("nurtureDuration")?.value || "60");
    const setupDiv = document.getElementById("nurtureSetup");
    const progressDiv = document.getElementById("nurtureProgress");
    const completeDiv = document.getElementById("nurtureComplete");
    const btnStart = document.getElementById("btnNurtureStart");
    const btnStop = document.getElementById("btnNurtureStop");
    const btnCancel = document.getElementById("btnNurtureCancel");
    const btnClose = document.getElementById("btnNurtureClose");
    if (setupDiv) setupDiv.style.display = "none";
    if (progressDiv) progressDiv.style.display = "block";
    if (completeDiv) completeDiv.style.display = "none";
    if (btnStart) btnStart.style.display = "none";
    if (btnStop) btnStop.style.display = "inline-block";
    if (btnCancel) btnCancel.style.display = "none";
    if (btnClose) btnClose.style.display = "none";
    nurtureStartTime = Date.now();
    nurtureTotalSeconds = seconds;
    nurtureAborted = false;
    nurtureInProgress = accountId;
    const taskId = `nurture-${Date.now()}`;
    currentNurtureTaskId = taskId;
    const nurtureTask = {
      id: taskId,
      type: "nurture",
      title: `\u{1F331} ${t("nurture.title")}: ${accountInfo}`,
      status: "running",
      progress: 0,
      total: seconds,
      createdAt: /* @__PURE__ */ new Date(),
      data: { accountId, platform, seconds }
    };
    tasks.unshift(nurtureTask);
    renderTasks();
    updateNurtureTimer();
    nurtureTimerInterval = window.setInterval(updateNurtureTimer, 1e3);
    try {
      const result = await invoke2("quick_nurture", {
        accountId,
        seconds
      });
      if (nurtureTimerInterval) {
        clearInterval(nurtureTimerInterval);
        nurtureTimerInterval = null;
      }
      if (!nurtureAborted) {
        const task = tasks.find((t2) => t2.id === taskId);
        if (task) {
          task.status = "completed";
          task.progress = seconds;
          task.completedAt = /* @__PURE__ */ new Date();
        }
        renderTasks();
        showNurtureComplete(seconds, result);
      }
      await loadAccounts();
    } catch (error) {
      console.error("Nurture failed:", error);
      if (nurtureTimerInterval) {
        clearInterval(nurtureTimerInterval);
        nurtureTimerInterval = null;
      }
      const task = tasks.find((t2) => t2.id === taskId);
      if (task) {
        task.status = "failed";
        task.error = String(error);
        task.completedAt = /* @__PURE__ */ new Date();
      }
      renderTasks();
      showToast(`${t("nurture.failed")}: ${error}`, "error");
      resetNurtureModal();
    } finally {
      nurtureInProgress = null;
    }
  };
  function updateNurtureTimer() {
    const elapsed = Math.floor((Date.now() - nurtureStartTime) / 1e3);
    const remaining = Math.max(0, nurtureTotalSeconds - elapsed);
    const progress = Math.min(100, elapsed / nurtureTotalSeconds * 100);
    const timerEl = document.getElementById("nurtureTimer");
    if (timerEl) {
      const mins = Math.floor(remaining / 60);
      const secs = remaining % 60;
      timerEl.textContent = `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
    }
    const progressBar = document.getElementById("nurtureProgressBar");
    if (progressBar) {
      progressBar.style.width = `${progress}%`;
    }
    const statusEl = document.getElementById("nurtureStatusText");
    if (statusEl) {
      const elapsedMins = Math.floor(elapsed / 60);
      const elapsedSecs = elapsed % 60;
      statusEl.innerHTML = `<span class="spinner-small"></span> ${t("nurture.running")} - ${elapsedMins}:${elapsedSecs.toString().padStart(2, "0")} / ${Math.floor(nurtureTotalSeconds / 60)}:${(nurtureTotalSeconds % 60).toString().padStart(2, "0")}`;
    }
    if (currentNurtureTaskId) {
      const task = tasks.find((t2) => t2.id === currentNurtureTaskId);
      if (task) {
        task.progress = elapsed;
        if (elapsed % 5 === 0) {
          renderTasks();
        }
      }
    }
  }
  function showNurtureComplete(seconds, result) {
    const setupDiv = document.getElementById("nurtureSetup");
    const progressDiv = document.getElementById("nurtureProgress");
    const completeDiv = document.getElementById("nurtureComplete");
    const btnStop = document.getElementById("btnNurtureStop");
    const btnClose = document.getElementById("btnNurtureClose");
    const summaryEl = document.getElementById("nurtureCompleteSummary");
    if (setupDiv) setupDiv.style.display = "none";
    if (progressDiv) progressDiv.style.display = "none";
    if (completeDiv) completeDiv.style.display = "block";
    if (btnStop) btnStop.style.display = "none";
    if (btnClose) btnClose.style.display = "inline-block";
    if (summaryEl) {
      const mins = Math.floor(seconds / 60);
      const secs = seconds % 60;
      summaryEl.textContent = currentLanguage === "zh" ? `\u672C\u6B21\u517B\u53F7 ${mins > 0 ? mins + " \u5206\u949F " : ""}${secs} \u79D2` : `Nurtured for ${mins > 0 ? mins + " min " : ""}${secs} sec`;
    }
    showToast(`${t("nurture.completed")}`, "success");
  }
  window.stopNurture = function() {
    nurtureAborted = true;
    if (nurtureTimerInterval) {
      clearInterval(nurtureTimerInterval);
      nurtureTimerInterval = null;
    }
    if (currentNurtureTaskId) {
      const task = tasks.find((t2) => t2.id === currentNurtureTaskId);
      if (task) {
        const elapsed = Math.floor((Date.now() - nurtureStartTime) / 1e3);
        task.status = "failed";
        task.error = t("nurture.stopped");
        task.progress = elapsed;
        task.completedAt = /* @__PURE__ */ new Date();
      }
      currentNurtureTaskId = null;
      renderTasks();
    }
    showToast(t("nurture.stopped"), "info");
    resetNurtureModal();
  };
  function resetNurtureModal() {
    const setupDiv = document.getElementById("nurtureSetup");
    const progressDiv = document.getElementById("nurtureProgress");
    const completeDiv = document.getElementById("nurtureComplete");
    const btnStart = document.getElementById("btnNurtureStart");
    const btnStop = document.getElementById("btnNurtureStop");
    const btnCancel = document.getElementById("btnNurtureCancel");
    const btnClose = document.getElementById("btnNurtureClose");
    const progressBar = document.getElementById("nurtureProgressBar");
    if (setupDiv) setupDiv.style.display = "block";
    if (progressDiv) progressDiv.style.display = "none";
    if (completeDiv) completeDiv.style.display = "none";
    if (btnStart) btnStart.style.display = "inline-block";
    if (btnStop) btnStop.style.display = "none";
    if (btnCancel) btnCancel.style.display = "inline-block";
    if (btnClose) btnClose.style.display = "none";
    if (progressBar) progressBar.style.width = "0%";
  }
  async function openBatchNurtureModal() {
    if (accounts.length === 0) {
      try {
        const data = await invoke2("list_accounts");
        accounts = data || [];
      } catch (error) {
        console.error("Failed to load accounts:", error);
      }
    }
    const container = document.getElementById("nurtureAccountList");
    if (!container) return;
    if (accounts.length === 0) {
      container.innerHTML = `<p class="text-muted">${t("nurture.noAccounts")}</p>`;
    } else {
      const platformIcons = {
        twitter: "\u{1F426}",
        linkedin: "\u{1F4BC}",
        reddit: "\u{1F916}",
        facebook: "\u{1F464}",
        devto: "\u{1F469}\u200D\u{1F4BB}",
        medium: "\u{1F4DD}",
        discord: "\u{1F4AC}",
        telegram: "\u2708\uFE0F",
        weibo: "\u{1F310}",
        xiaohongshu: "\u{1F4D5}",
        douyin: "\u{1F3B5}",
        bilibili: "\u{1F4FA}"
      };
      container.innerHTML = accounts.map((account) => {
        const icon = platformIcons[account.platform] || "\u{1F4F1}";
        const name = account.username || account.email || "N/A";
        const nurtureStats = account.total_nurture_seconds > 0 ? ` (\u7D2F\u8BA1: ${formatNurtureTime(account.total_nurture_seconds)})` : "";
        return `
        <label class="checkbox nurture-account-item">
          <input type="checkbox" name="nurtureAccount" value="${account.id}">
          <span>${icon} ${account.platform} - ${escapeHtml(name)}${nurtureStats}</span>
        </label>
      `;
      }).join("");
    }
    openModal("modalBatchNurture");
  }
  window.selectAllNurtureAccounts = function() {
    document.querySelectorAll('input[name="nurtureAccount"]').forEach((cb) => {
      cb.checked = true;
    });
  };
  window.deselectAllNurtureAccounts = function() {
    document.querySelectorAll('input[name="nurtureAccount"]').forEach((cb) => {
      cb.checked = false;
    });
  };
  window.startBatchNurtureTask = function() {
    const selectedAccounts = Array.from(
      document.querySelectorAll('input[name="nurtureAccount"]:checked')
    ).map((cb) => cb.value);
    if (selectedAccounts.length === 0) {
      showToast("Please select at least one account / \u8BF7\u81F3\u5C11\u9009\u62E9\u4E00\u4E2A\u8D26\u53F7", "error");
      return;
    }
    const duration = parseInt(document.getElementById("batchNurtureDuration")?.value || "60");
    invoke2("enqueue_nurture", { accountIds: selectedAccounts, duration }).then((n) => {
      showToast(`\u5DF2\u5165\u961F ${n} \u6761\u517B\u53F7\u4EFB\u52A1\uFF0C\u5F15\u64CE\u5C06\u81EA\u52A8\u6267\u884C\uFF08\u672A\u7ED1\u5B9A profile \u7684\u8D26\u53F7\u5DF2\u8DF3\u8FC7\uFF09`, n > 0 ? "success" : "error");
      if (currentPage === "tasks") refreshTasksPage();
    }).catch((e) => showToast("\u5165\u961F\u5931\u8D25: " + e, "error"));
    closeModal("modalBatchNurture");
  };
  function formatNurtureTime(seconds) {
    if (seconds < 60) return `${seconds} ${t("nurture.seconds")}`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)} min`;
    const hours = seconds / 3600;
    if (hours < 24) return `${hours.toFixed(1)} ${t("nurture.hours")}`;
    return `${(hours / 24).toFixed(1)} ${t("nurture.days")}`;
  }
  function formatTimeAgo(dateStr) {
    const date = new Date(dateStr);
    const now = /* @__PURE__ */ new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 6e4);
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }
  async function loadScheduledJobs() {
    try {
      scheduledJobs = await invoke2("unzoo_list_scheduled_jobs");
      renderScheduledJobs();
    } catch (error) {
      console.error("Failed to load scheduled jobs:", error);
      scheduledJobs = [];
    }
  }
  function renderScheduledJobs() {
    const container = document.getElementById("scheduledJobsList");
    if (!container) return;
    if (scheduledJobs.length === 0) {
      container.innerHTML = '<p class="text-muted">No scheduled jobs</p>';
      return;
    }
    container.innerHTML = scheduledJobs.map((job) => `
    <div class="scheduled-job-item ${job.enabled ? "" : "disabled"}">
      <div class="job-info">
        <span class="job-name">${escapeHtml(job.name)}</span>
        <span class="job-schedule">${escapeHtml(job.schedule)}</span>
        <span class="job-type">${escapeHtml(job.task_type)}</span>
      </div>
      <div class="job-actions">
        <button class="btn btn-small btn-secondary" onclick="toggleScheduledJob('${job.id}', ${!job.enabled})">${job.enabled ? "Pause" : "Resume"}</button>
        <button class="btn btn-small btn-danger" onclick="deleteScheduledJob('${job.id}')">Delete</button>
      </div>
    </div>
  `).join("");
  }
  window.toggleScheduledJob = async function(jobId, enabled) {
    try {
      if (enabled) {
        await invoke2("unzoo_resume_scheduled_job", { jobId });
      } else {
        await invoke2("unzoo_pause_scheduled_job", { jobId });
      }
      showToast(enabled ? "Job resumed" : "Job paused", "success");
      await loadScheduledJobs();
    } catch (error) {
      console.error("Failed to toggle job:", error);
      showToast("Failed to toggle job", "error");
    }
  };
  window.deleteScheduledJob = async function(jobId) {
    if (!confirm("Delete this scheduled job?")) return;
    try {
      await invoke2("unzoo_delete_scheduled_job", { jobId });
      showToast("Job deleted", "success");
      await loadScheduledJobs();
    } catch (error) {
      console.error("Failed to delete job:", error);
      showToast("Failed to delete job", "error");
    }
  };
  async function createScheduledJob() {
    const name = document.getElementById("jobName")?.value;
    const schedule = document.getElementById("jobSchedule")?.value;
    const taskType = document.getElementById("jobTaskType")?.value;
    const taskData = document.getElementById("jobTaskData")?.value;
    if (!name || !schedule || !taskType) {
      showToast("Name, schedule and task type are required", "error");
      return;
    }
    try {
      let parsedData = {};
      if (taskData) {
        try {
          parsedData = JSON.parse(taskData);
        } catch {
          showToast("Invalid JSON in task data", "error");
          return;
        }
      }
      await invoke2("unzoo_create_scheduled_job", {
        name,
        schedule,
        taskType,
        taskData: parsedData
      });
      closeModal("modalAddSchedule");
      showToast("Scheduled job created", "success");
      await loadScheduledJobs();
    } catch (error) {
      console.error("Failed to create job:", error);
      showToast("Failed to create job", "error");
    }
  }
  var publishStrategies = [];
  var publishSelectedProducts = /* @__PURE__ */ new Set();
  async function loadPublishPage() {
    if (selectedProductIds.size > 0) {
      publishSelectedProducts = new Set(selectedProductIds);
    }
    renderPublishProductsList();
    await loadPublishStrategies();
  }
  function renderPublishProductsList() {
    const container = document.getElementById("publishProductsList");
    if (!container) return;
    if (products.length === 0) {
      container.innerHTML = '<p class="text-muted">No products. Add products first.</p>';
      return;
    }
    container.innerHTML = products.map((p) => {
      const isSelected = publishSelectedProducts.has(p.id);
      return `
      <label class="checkbox publish-product-item ${isSelected ? "selected" : ""}">
        <input type="checkbox" value="${p.id}" ${isSelected ? "checked" : ""} onchange="togglePublishProduct('${p.id}', this.checked)">
        <span>${escapeHtml(p.name)}</span>
      </label>
    `;
    }).join("");
    updatePublishProductsCount();
  }
  window.togglePublishProduct = function(id, checked) {
    if (checked) {
      publishSelectedProducts.add(id);
    } else {
      publishSelectedProducts.delete(id);
    }
    const items = document.querySelectorAll("#publishProductsList .publish-product-item");
    items.forEach((item) => {
      const checkbox = item.querySelector('input[type="checkbox"]');
      if (checkbox && checkbox.value === id) {
        item.classList.toggle("selected", checked);
      }
    });
    updatePublishProductsCount();
  };
  function updatePublishProductsCount() {
    const countEl = document.getElementById("publishProductsCount");
    if (countEl) {
      countEl.textContent = `${publishSelectedProducts.size} selected`;
    }
  }
  function selectAllPublishProducts() {
    products.forEach((p) => publishSelectedProducts.add(p.id));
    renderPublishProductsList();
  }
  function clearPublishProducts() {
    publishSelectedProducts.clear();
    renderPublishProductsList();
  }
  async function loadPublishStrategies() {
    try {
      publishStrategies = await invoke2("get_publish_strategies");
      renderStrategies();
    } catch (error) {
      console.error("Failed to load publish strategies:", error);
    }
  }
  function renderStrategies() {
    const grid = document.getElementById("strategiesGrid");
    if (!grid || publishStrategies.length === 0) return;
    grid.innerHTML = publishStrategies.map((s) => {
      let statusClass = "strategy-ok";
      let statusIcon = "\u2705";
      let statusText = "Ready to post";
      if (s.is_warming_up && s.max_daily === 0) {
        statusClass = "strategy-warmup";
        statusIcon = "\u{1F525}";
        statusText = `Warmup (${s.warmup_days_left} days left)`;
      } else if (!s.can_post_now) {
        if (s.wait_minutes < 0) {
          statusClass = "strategy-limit";
          statusIcon = "\u{1F6AB}";
          statusText = `Limit reached (${s.posts_today}/${s.max_daily})`;
        } else {
          statusClass = "strategy-wait";
          statusIcon = "\u23F3";
          statusText = `Wait ${s.wait_minutes} min`;
        }
      } else if (s.is_warming_up) {
        statusClass = "strategy-warmup";
        statusIcon = "\u{1F525}";
        statusText = `Warmup: ${s.posts_today}/${s.max_daily}`;
      }
      return `
      <div class="strategy-card ${statusClass}">
        <div class="strategy-header">
          <span class="strategy-platform">${escapeHtml(s.platform)}</span>
          <span class="strategy-status">${statusIcon}</span>
        </div>
        <div class="strategy-info">
          <div>${statusText}</div>
          <div class="strategy-meta">Today: ${s.posts_today}/${s.max_daily} | Interval: ${s.min_interval}min</div>
        </div>
      </div>
    `;
    }).join("");
    updatePlatformCheckboxes();
  }
  function updatePlatformCheckboxes() {
    const platformsGroup = document.getElementById("platformsGroup");
    if (!platformsGroup) return;
    platformsGroup.querySelectorAll('input[type="checkbox"]').forEach((cb) => {
      const strategy = publishStrategies.find((s) => s.platform === cb.value);
      const label = cb.parentElement;
      if (strategy) {
        if (strategy.can_post_now) {
          cb.disabled = false;
          label.style.opacity = "1";
          label.title = "Ready to post";
        } else {
          cb.disabled = true;
          cb.checked = false;
          label.style.opacity = "0.5";
          if (strategy.wait_minutes < 0) {
            label.title = `Daily limit reached (${strategy.posts_today}/${strategy.max_daily})`;
          } else {
            label.title = `Wait ${strategy.wait_minutes} minutes`;
          }
        }
      }
    });
  }
  async function generateContent() {
    const productIds = Array.from(publishSelectedProducts);
    if (productIds.length === 0) {
      showToast(t("msg.selectProduct"), "error");
      return;
    }
    const platforms = Array.from(document.querySelectorAll("#platformsGroup input:checked")).map((cb) => cb.value);
    const languages = Array.from(document.querySelectorAll("#languagesGroup input:checked")).map((cb) => cb.value);
    if (platforms.length === 0 || languages.length === 0) {
      showToast("Select at least one platform and language", "error");
      return;
    }
    const btn = document.getElementById("btnGenerate");
    if (btn) {
      btn.disabled = true;
      btn.textContent = "Generating...";
    }
    try {
      generatedContents = [];
      const total = productIds.length;
      let current = 0;
      for (const productId of productIds) {
        current++;
        const product = products.find((p) => p.id === productId);
        if (btn) btn.textContent = `Generating (${current}/${total})...`;
        const contents = await invoke2("generate_content", { productId, platforms, languages });
        generatedContents.push(...contents);
      }
      renderPreview();
      document.getElementById("btnPublish").disabled = false;
      showToast(`Generated ${generatedContents.length} contents for ${productIds.length} products`, "success");
    } catch (error) {
      console.error("Failed to generate content:", error);
      showToast("Failed to generate content", "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = "Generate Content";
      }
    }
  }
  function renderPreview() {
    const tabs = document.getElementById("previewTabs");
    const content = document.getElementById("previewContent");
    if (!tabs || !content || generatedContents.length === 0) return;
    tabs.innerHTML = generatedContents.map((c, i) => {
      const shortName = c.product_name ? c.product_name.substring(0, 12) : "Product";
      return `<button class="preview-tab ${i === 0 ? "active" : ""}" data-index="${i}" title="${c.product_name || ""} - ${c.platform} (${c.language})">${shortName} \xB7 ${c.platform}</button>`;
    }).join("");
    tabs.querySelectorAll(".preview-tab").forEach((tab) => {
      tab.addEventListener("click", () => {
        tabs.querySelectorAll(".preview-tab").forEach((t2) => t2.classList.remove("active"));
        tab.classList.add("active");
        const idx = parseInt(tab.dataset.index || "0");
        showPreviewContent(idx);
      });
    });
    showPreviewContent(0);
  }
  function showPreviewContent(index) {
    const content = document.getElementById("previewContent");
    if (!content || !generatedContents[index]) return;
    const c = generatedContents[index];
    content.innerHTML = `
    <div class="preview-meta">
      <span class="preview-product">${escapeHtml(c.product_name || "Product")}</span>
      <span class="preview-platform">${escapeHtml(c.platform)}</span>
      <span class="preview-lang">${escapeHtml(c.language)}</span>
    </div>
    <div class="preview-body">${escapeHtml(c.body)}</div>
    <div class="preview-hashtags">${c.hashtags.map((h) => `#${h}`).join(" ")}</div>
  `;
  }
  function simulatePublish() {
    showToast("Simulation mode - no actual posts", "info");
  }
  var currentPreviewIndex = 0;
  var pendingPublishContents = [];
  async function prepareAndPreview(index) {
    if (index >= pendingPublishContents.length) {
      showToast(t("msg.allPublished"), "success");
      resetPublishUI();
      return;
    }
    const content = pendingPublishContents[index];
    currentPreviewIndex = index;
    const btn = document.getElementById("btnPublish");
    const simBtn = document.getElementById("btnSimulate");
    if (btn) {
      btn.disabled = true;
      btn.textContent = "Preparing...";
    }
    if (simBtn) simBtn.disabled = true;
    const previewContent = document.getElementById("previewContent");
    if (previewContent) {
      previewContent.innerHTML = `
      <div class="publish-preview">
        <div class="preview-header">
          <span class="preview-step">Step ${index + 1} of ${pendingPublishContents.length}</span>
          <span class="preview-platform-badge">${escapeHtml(content.platform)}</span>
        </div>
        <div class="preview-status preparing">
          <div class="spinner"></div>
          <span>Preparing content for ${content.platform}...</span>
        </div>
      </div>
    `;
    }
    try {
      const preview = await invoke2("prepare_publish", { content });
      if (previewContent) {
        if (preview.ready) {
          previewContent.innerHTML = `
          <div class="publish-preview">
            <div class="preview-header">
              <span class="preview-step">Step ${index + 1} of ${pendingPublishContents.length}</span>
              <span class="preview-platform-badge">${escapeHtml(content.platform)}</span>
              <span class="preview-lang-badge">${escapeHtml(content.language)}</span>
            </div>

            ${preview.screenshot ? `
              <div class="preview-screenshot">
                <img src="data:image/png;base64,${preview.screenshot}" alt="Preview" />
              </div>
            ` : ""}

            <div class="preview-content-box">
              <div class="preview-label">Content to be posted:</div>
              <div class="preview-text">${escapeHtml(content.body)}</div>
            </div>

            <div class="preview-warning">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
              </svg>
              <span>Please verify the content in the browser before confirming.</span>
            </div>

            <div class="preview-actions">
              <button class="btn-secondary" onclick="skipCurrentPublish()">
                Skip
              </button>
              <button class="btn-danger" onclick="cancelAllPublish()">
                Cancel All
              </button>
              <button class="btn-primary" onclick="confirmCurrentPublish()">
                \u2713 Confirm & Publish
              </button>
            </div>
          </div>
        `;
        } else {
          previewContent.innerHTML = `
          <div class="publish-preview">
            <div class="preview-header">
              <span class="preview-step">Step ${index + 1} of ${pendingPublishContents.length}</span>
              <span class="preview-platform-badge error">${escapeHtml(content.platform)}</span>
            </div>

            <div class="preview-error">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
              </svg>
              <span>${escapeHtml(preview.error || "Unknown error")}</span>
            </div>

            <div class="preview-actions">
              <button class="btn-secondary" onclick="skipCurrentPublish()">
                Skip & Continue
              </button>
              <button class="btn-danger" onclick="cancelAllPublish()">
                Cancel All
              </button>
            </div>
          </div>
        `;
        }
      }
      if (btn) {
        btn.disabled = false;
        btn.textContent = "Publish All";
      }
    } catch (error) {
      if (previewContent) {
        previewContent.innerHTML = `
        <div class="publish-preview">
          <div class="preview-error">
            <span>Error: ${error}</span>
          </div>
          <div class="preview-actions">
            <button class="btn-secondary" onclick="skipCurrentPublish()">Skip</button>
            <button class="btn-danger" onclick="cancelAllPublish()">Cancel</button>
          </div>
        </div>
      `;
      }
    }
  }
  async function confirmCurrentPublish() {
    const content = pendingPublishContents[currentPreviewIndex];
    const previewContent = document.getElementById("previewContent");
    if (previewContent) {
      previewContent.innerHTML = `
      <div class="publish-preview">
        <div class="preview-status publishing">
          <div class="spinner"></div>
          <span>Publishing to ${content.platform}...</span>
        </div>
      </div>
    `;
    }
    try {
      const result = await invoke2("confirm_publish", {
        platform: content.platform,
        productId: content.product_id,
        contentBody: content.body
      });
      if (result.success) {
        showToast(`Published to ${content.platform}!`, "success");
      } else {
        showToast(`Failed: ${result.error}`, "error");
      }
    } catch (error) {
      showToast(`Error: ${error}`, "error");
    }
    await prepareAndPreview(currentPreviewIndex + 1);
  }
  async function skipCurrentPublish() {
    const content = pendingPublishContents[currentPreviewIndex];
    try {
      await invoke2("cancel_publish", { platform: content.platform });
    } catch (e) {
    }
    showToast(`Skipped ${content.platform}`, "info");
    await prepareAndPreview(currentPreviewIndex + 1);
  }
  async function cancelAllPublish() {
    const content = pendingPublishContents[currentPreviewIndex];
    try {
      await invoke2("cancel_publish", { platform: content.platform });
    } catch (e) {
    }
    pendingPublishContents = [];
    resetPublishUI();
    showToast("Publishing cancelled", "info");
  }
  function resetPublishUI() {
    const btn = document.getElementById("btnPublish");
    const simBtn = document.getElementById("btnSimulate");
    if (btn) {
      btn.disabled = false;
      btn.textContent = "Publish All";
    }
    if (simBtn) simBtn.disabled = false;
    if (generatedContents.length > 0) {
      showPreviewContent(0);
    }
  }
  window.confirmCurrentPublish = confirmCurrentPublish;
  window.skipCurrentPublish = skipCurrentPublish;
  window.cancelAllPublish = cancelAllPublish;
  var keywords = [];
  var discoveredPosts = [];
  var currentReplyPost = null;
  var engageInboxWired = false;
  async function loadEngagePage() {
    if (!engageInboxWired) {
      engageInboxWired = true;
      document.getElementById("btnInboxRefresh")?.addEventListener("click", () => loadEngageInbox());
      document.getElementById("inboxFilter")?.addEventListener("change", () => loadEngageInbox());
      document.getElementById("engageAutoToggle")?.addEventListener("change", async (e) => {
        const on = e.target.checked;
        try {
          await invoke2("engage_set_auto", { on, intervalMinutes: null, maxInflight: null });
          showToast(on ? "\u5DF2\u5F00\u542F\u81EA\u52A8\u83B7\u5BA2" : "\u5DF2\u5173\u95ED\u81EA\u52A8\u83B7\u5BA2", "success");
          loadEngageControl();
        } catch (err) {
          showToast("" + err, "error");
        }
      });
      document.getElementById("engageSaveCfg")?.addEventListener("click", async () => {
        const on = document.getElementById("engageAutoToggle")?.checked ?? true;
        const intervalMinutes = parseInt(document.getElementById("engageInterval")?.value || "30", 10);
        const maxInflight = parseInt(document.getElementById("engageMaxInflight")?.value || "6", 10);
        try {
          await invoke2("engage_set_auto", { on, intervalMinutes, maxInflight });
          showToast("\u5DF2\u4FDD\u5B58\u5DE1\u68C0\u8282\u594F", "success");
          loadEngageControl();
        } catch (err) {
          showToast("" + err, "error");
        }
      });
    }
    loadEngageControl();
    loadEngageInbox();
    await loadKeywords();
    await loadReplyStrategies();
    await loadDiscoveredPosts();
    const select = document.getElementById("keywordProduct");
    if (select) {
      select.innerHTML = '<option value="">No specific product</option>' + products.map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
    }
    const replySelect = document.getElementById("replyProduct");
    if (replySelect) {
      replySelect.innerHTML = '<option value="">No product (generic reply)</option>' + products.map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
    }
  }
  async function loadEngageControl() {
    try {
      const s = await invoke2("engage_get_settings");
      const toggle = document.getElementById("engageAutoToggle");
      const label = document.getElementById("engageAutoLabel");
      const interval = document.getElementById("engageInterval");
      const maxIn = document.getElementById("engageMaxInflight");
      const status = document.getElementById("engageStatus");
      if (toggle) toggle.checked = !!s.auto;
      if (label) label.textContent = s.auto ? "\u2705 \u81EA\u52A8\u83B7\u5BA2\u8FD0\u884C\u4E2D" : "\u5DF2\u5173\u95ED\uFF08\u70B9\u5F00\u542F\u8BA9\u5F15\u64CE\u81EA\u5DF1\u5E72\uFF09";
      if (interval && document.activeElement !== interval) interval.value = String(s.interval_minutes || 30);
      if (maxIn && document.activeElement !== maxIn) maxIn.value = String(s.max_inflight || 6);
      const modeLabel = s.reply_mode === "auto" ? "\u{1F7E2}\u5168\u81EA\u52A8(\u771F\u53D1)" : "\u{1F7E1}\u534A\u81EA\u52A8(\u8FDB\u6536\u4EF6\u7BB1\u5F85\u5BA1)";
      const last = s.last_tick ? new Date(s.last_tick).toLocaleTimeString() : "\u5C1A\u672A\u5DE1\u68C0";
      if (status) status.innerHTML = `\u6A21\u5F0F <b>${modeLabel}</b> \xB7 \u542F\u7528\u5173\u952E\u8BCD <b>${s.keywords_enabled}</b> \xB7 \u5728\u8DD1 <b>${s.inflight}</b> \xB7 \u4E0A\u6B21\u5DE1\u68C0 ${last}`;
    } catch (e) {
    }
  }
  async function loadEngageInbox() {
    const box = document.getElementById("inboxList");
    const sum = document.getElementById("engageSummary");
    if (!box) return;
    const filter = document.getElementById("inboxFilter")?.value || "all";
    try {
      const s = await invoke2("engage_summary");
      if (sum) sum.innerHTML = `\u{1F525}\u5F85\u5BA1 <b>${s.pending_review}</b> \xB7 \u7EBF\u7D22 <b>${s.leads_open}</b> \xB7 \u2705\u8F6C\u5316 <b>${s.converted}</b> \xB7 \u63D0\u53CA <b>${s.mentions}</b>`;
    } catch {
    }
    let items = [];
    try {
      items = await invoke2("engage_inbox", { filter }) || [];
    } catch (e) {
      box.innerHTML = `<p class="text-muted">\u52A0\u8F7D\u5931\u8D25\uFF1A${escapeHtml("" + e)}</p>`;
      return;
    }
    if (!items.length) {
      box.innerHTML = '<p class="text-muted">\u6682\u65E0\u4E92\u52A8\u3002\u5F00\u542F\u5173\u952E\u8BCD\u53D1\u73B0 + \u5168\u81EA\u52A8\u540E\uFF0C\u8FD9\u91CC\u4F1A\u81EA\u52A8\u6C47\u96C6\u7EBF\u7D22\u4E0E\u5F85\u5BA1\u56DE\u590D\u3002</p>';
      return;
    }
    const kindLabel = { lead: "\u7EBF\u7D22", pending_reply: "\u5F85\u5BA1\u56DE\u590D", mention: "\u54C1\u724C\u63D0\u53CA" };
    box.innerHTML = items.map((it) => {
      const hot = it.hot ? `<span style="background:#ff4757;color:#fff;border-radius:8px;padding:1px 6px;font-size:10px;">\u{1F525}\u5F3A\u610F\u5411</span>` : "";
      const intentColor = it.intent >= 70 ? "#ff4757" : it.intent >= 40 ? "#ffa502" : "#999";
      const link = it.url ? `<a href="${escapeHtml(it.url)}" target="_blank" class="btn btn-small btn-secondary">\u6253\u5F00\u2197</a>` : "";
      let actions = link;
      if (it.kind === "pending_reply") {
        actions += ` <button class="btn btn-small btn-primary" onclick="inboxApprove('${it.ref_id}')">\u2705\u901A\u8FC7\u53D1\u5E03</button>
                   <button class="btn btn-small btn-secondary" onclick="inboxReject('${it.ref_id}')">\u5FFD\u7565</button>`;
      } else if (it.kind === "lead") {
        actions += ` <button class="btn btn-small btn-primary" onclick="inboxLead('${it.ref_id}','converted')">\u2705\u5DF2\u8F6C\u5316</button>
                   <button class="btn btn-small btn-secondary" onclick="inboxLead('${it.ref_id}','dismissed')">\u5FFD\u7565</button>`;
      }
      return `<div class="card" style="padding:10px 12px;margin-bottom:8px;border-left:3px solid ${it.hot ? "#ff4757" : "transparent"};">
      <div style="display:flex;justify-content:space-between;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:4px;">
        <div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;">
          <span class="badge">${kindLabel[it.kind] || it.kind}</span>
          <span class="badge">${escapeHtml(it.platform)}</span>
          ${it.author ? `<span class="text-muted" style="font-size:12px;">@${escapeHtml(it.author)}</span>` : ""}
          ${hot}
          <span style="color:${intentColor};font-size:11px;font-weight:600;">\u610F\u5411 ${it.intent}</span>
        </div>
        <span class="text-muted" style="font-size:11px;">${new Date(it.created_at).toLocaleString()}</span>
      </div>
      <div style="font-size:13px;white-space:pre-wrap;margin-bottom:6px;">${escapeHtml((it.text || "").slice(0, 240))}</div>
      <div class="btn-group" style="gap:6px;flex-wrap:wrap;">${actions}</div>
    </div>`;
    }).join("");
  }
  window.inboxApprove = async (id) => {
    try {
      await invoke2("approve_reply", { id, editedContent: null });
      showToast("\u5DF2\u5165\u961F\u53D1\u5E03", "success");
      loadEngageInbox();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.inboxReject = async (id) => {
    try {
      await invoke2("reject_reply", { id });
      showToast("\u5DF2\u5FFD\u7565", "success");
      loadEngageInbox();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.inboxLead = async (id, status) => {
    try {
      await invoke2("update_lead_status", { id, status, notes: null });
      showToast("\u5DF2\u66F4\u65B0", "success");
      loadEngageInbox();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  async function loadKeywords() {
    try {
      keywords = await invoke2("list_keywords");
      renderKeywords();
    } catch (error) {
      console.error("Failed to load keywords:", error);
    }
  }
  function renderKeywords() {
    const container = document.getElementById("keywordsList");
    if (!container) return;
    if (keywords.length === 0) {
      container.innerHTML = '<p class="text-muted">No keywords yet. Add keywords to start discovering relevant posts.</p>';
      return;
    }
    container.innerHTML = keywords.map((k) => `
    <div class="keyword-item">
      <span class="keyword-text">${escapeHtml(k.keyword)}</span>
      <span class="keyword-platforms">${k.platforms.join(", ")}</span>
      <button class="keyword-delete" onclick="deleteKeyword('${k.id}')">\xD7</button>
    </div>
  `).join("");
  }
  async function loadReplyStrategies() {
    try {
      const strategies = await invoke2("get_reply_strategies");
      renderReplyStrategies(strategies);
    } catch (error) {
      console.error("Failed to load reply strategies:", error);
    }
  }
  function renderReplyStrategies(strategies) {
    const grid = document.getElementById("replyStrategiesGrid");
    if (!grid || strategies.length === 0) return;
    grid.innerHTML = strategies.map((s) => {
      let statusClass = "strategy-ok";
      let statusIcon = "\u2705";
      let statusText = "Ready to reply";
      if (!s.can_reply_now) {
        if (s.wait_minutes < 0) {
          statusClass = "strategy-limit";
          statusIcon = "\u{1F6AB}";
          statusText = `Limit: ${s.replies_today}/${s.max_daily}`;
        } else {
          statusClass = "strategy-wait";
          statusIcon = "\u23F3";
          statusText = `Wait ${s.wait_minutes} min`;
        }
      }
      return `
      <div class="strategy-card ${statusClass}">
        <div class="strategy-header">
          <span class="strategy-platform">${escapeHtml(s.platform)}</span>
          <span class="strategy-status">${statusIcon}</span>
        </div>
        <div class="strategy-info">
          <div>${statusText}</div>
          <div class="strategy-meta">
            Today: ${s.replies_today}/${s.max_daily} |
            \u{1F4EC} ${s.discovered_posts} posts
          </div>
        </div>
      </div>
    `;
    }).join("");
  }
  async function loadDiscoveredPosts() {
    try {
      const platform = document.getElementById("filterPlatform")?.value || void 0;
      discoveredPosts = await invoke2("list_discovered_posts", { platform, status: "new" });
      renderDiscoveredPosts();
    } catch (error) {
      console.error("Failed to load discovered posts:", error);
    }
  }
  function renderDiscoveredPosts() {
    const container = document.getElementById("discoveredPosts");
    if (!container) return;
    if (discoveredPosts.length === 0) {
      container.innerHTML = '<p class="text-muted">No posts discovered yet. Add keywords and click "Discover Now".</p>';
      return;
    }
    container.innerHTML = discoveredPosts.map((p) => `
    <div class="post-item" data-id="${p.id}">
      <div class="post-header">
        <span class="post-platform">${escapeHtml(p.platform)}</span>
        <span class="post-relevance">${Math.round(p.relevance_score * 100)}% match</span>
      </div>
      <div class="post-title">${escapeHtml(p.post_title || "Untitled")}</div>
      <div class="post-url">${escapeHtml(p.post_url)}</div>
      ${p.keyword_matched ? `<span class="post-keyword">\u{1F50D} ${escapeHtml(p.keyword_matched)}</span>` : ""}
      <div class="post-actions">
        <button class="btn btn-small btn-primary" onclick="openReplyModal('${p.id}')">\u{1F4AC} Reply</button>
        <button class="btn btn-small btn-secondary" onclick="viewPost('${p.post_url}')">\u{1F441} View</button>
        <button class="btn btn-small btn-secondary" onclick="skipPost('${p.id}')">Skip</button>
      </div>
    </div>
  `).join("");
  }
  function openKeywordModal() {
    document.getElementById("keywordText").value = "";
    document.getElementById("keywordProduct").value = "";
    document.querySelectorAll("#keywordPlatforms input").forEach((cb) => cb.checked = cb.value === "reddit" || cb.value === "twitter");
    openModal("modalAddKeyword");
  }
  async function saveKeyword() {
    const keyword = document.getElementById("keywordText")?.value?.trim();
    const productId = document.getElementById("keywordProduct")?.value || null;
    const platforms = Array.from(document.querySelectorAll("#keywordPlatforms input:checked")).map((cb) => cb.value);
    if (!keyword) {
      showToast("Please enter a keyword", "error");
      return;
    }
    if (platforms.length === 0) {
      showToast(t("msg.selectPlatform"), "error");
      return;
    }
    try {
      await invoke2("add_keyword", { keyword, productId, platforms });
      closeModal("modalAddKeyword");
      await loadKeywords();
      showToast(t("msg.keywordAdded"), "success");
    } catch (error) {
      showToast("Failed to add keyword: " + error, "error");
    }
  }
  window.deleteKeyword = async function(id) {
    if (!confirm("Delete this keyword?")) return;
    try {
      await invoke2("delete_keyword", { id });
      await loadKeywords();
      showToast(t("msg.keywordDeleted"), "success");
    } catch (error) {
      showToast("Failed to delete keyword", "error");
    }
  };
  async function discoverPosts() {
    if (keywords.length === 0) {
      showToast(t("msg.addKeywordsFirst"), "error");
      return;
    }
    const btn = document.getElementById("btnDiscoverNow");
    if (btn) {
      btn.disabled = true;
      btn.textContent = "Discovering...";
    }
    let totalFound = 0;
    for (const k of keywords) {
      for (const platform of k.platforms) {
        try {
          const posts = await invoke2("discover_posts", { platform, keyword: k.keyword });
          totalFound += posts.length;
        } catch (error) {
          console.error(`Failed to discover on ${platform}:`, error);
        }
        await new Promise((resolve) => setTimeout(resolve, 2e3));
      }
    }
    if (btn) {
      btn.disabled = false;
      btn.textContent = "\u{1F50D} Discover Now";
    }
    await loadDiscoveredPosts();
    await loadReplyStrategies();
    showToast(`Discovered ${totalFound} new posts`, "success");
  }
  window.openReplyModal = function(postId) {
    currentReplyPost = discoveredPosts.find((p) => p.id === postId);
    if (!currentReplyPost) return;
    const postInfo = document.getElementById("replyPostInfo");
    if (postInfo) {
      postInfo.innerHTML = `
      <span class="post-platform">${escapeHtml(currentReplyPost.platform)}</span>
      <div style="margin-top: 8px; font-weight: 500;">${escapeHtml(currentReplyPost.post_title || "Untitled")}</div>
      <div style="font-size: 0.8rem; color: var(--text-muted); margin-top: 4px;">${escapeHtml(currentReplyPost.post_url)}</div>
    `;
    }
    document.getElementById("replyContent").value = "";
    openModal("modalReply");
  };
  window.viewPost = function(url) {
    window.open(url, "_blank");
  };
  window.skipPost = async function(postId) {
    try {
      await invoke2("update_post_status", { postId, status: "skipped" });
      await loadDiscoveredPosts();
      showToast("Post skipped", "info");
    } catch (error) {
      showToast("Failed to update post", "error");
    }
  };
  async function generateReplyContent() {
    if (!currentReplyPost) return;
    const productId = document.getElementById("replyProduct")?.value;
    const product = products.find((p) => p.id === productId);
    const generateBtn = document.getElementById("btnGenerateReply");
    try {
      if (generateBtn) {
        generateBtn.disabled = true;
        generateBtn.textContent = "Generating...";
      }
      const reply = await invoke2("generate_ai_reply", {
        postTitle: currentReplyPost.post_title || "",
        postContent: currentReplyPost.post_content || "",
        keyword: currentReplyPost.keyword_matched || "",
        productName: product?.name || null,
        productTagline: product?.tagline || null
      });
      document.getElementById("replyContent").value = reply;
      showToast(t("msg.replyGenerated"), "success");
    } catch (error) {
      console.error("Failed to generate reply:", error);
      const templates = [
        `I've been using ${product?.name || "a similar tool"} for this. ${product?.tagline || "It works well"}. Worth checking out!`,
        `Have you tried ${product?.name || "looking into alternatives"}? ${product?.tagline || "Might help with your use case"}.`
      ];
      const reply = templates[Math.floor(Math.random() * templates.length)];
      document.getElementById("replyContent").value = reply;
      showToast("Using template (AI unavailable)", "warning");
    } finally {
      if (generateBtn) {
        generateBtn.disabled = false;
        generateBtn.textContent = "\u2728 Generate with AI";
      }
    }
  }
  async function sendReply() {
    if (!currentReplyPost) return;
    const replyContent = document.getElementById("replyContent")?.value?.trim();
    const productId = document.getElementById("replyProduct")?.value || void 0;
    if (!replyContent) {
      showToast(t("msg.pleaseWriteReply"), "error");
      return;
    }
    const btn = document.getElementById("btnSendReply");
    if (btn) {
      btn.disabled = true;
      btn.textContent = "Sending...";
    }
    try {
      const result = await invoke2("reply_to_post", {
        postId: currentReplyPost.id,
        productId,
        customReply: replyContent
      });
      if (result.success) {
        closeModal("modalReply");
        await loadDiscoveredPosts();
        await loadReplyStrategies();
        showToast(t("msg.replySent"), "success");
      } else {
        showToast(result.error || "Failed to send reply", "error");
      }
    } catch (error) {
      showToast("Failed to send reply: " + error, "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = "Send Reply";
      }
    }
  }
  async function loadStats() {
    try {
      const days = parseInt(document.getElementById("statsTimeRange")?.value || "30");
      const stats = await invoke2("get_detailed_stats", { days });
      updateOverviewStats(stats);
      renderActivityChart(stats.daily_data || []);
      renderPlatformPerformance(stats.platform_stats || {});
      renderContentBreakdown(stats.content_breakdown || {});
      renderBestContent(stats.best_content || []);
      renderActivityHeatmap(stats.heatmap_data || []);
      initStatsEvents();
    } catch (error) {
      console.error("Stats error:", error);
      setDefaultStatsUI();
    }
  }
  function updateOverviewStats(stats) {
    document.getElementById("statPosts").textContent = formatNumber(stats.total_posts || 0);
    document.getElementById("statViews").textContent = formatNumber(stats.total_views || 0);
    document.getElementById("statEngagements").textContent = formatNumber(stats.total_engagements || 0);
    document.getElementById("statRate").textContent = ((stats.avg_engagement_rate || 0) * 100).toFixed(1) + "%";
    const postsChange = stats.posts_change || 0;
    const viewsChange = stats.views_change || 0;
    const engChange = stats.engagements_change || 0;
    const rateChange = stats.rate_change || 0;
    setChangeIndicator("statPostsChange", postsChange);
    setChangeIndicator("statViewsChange", viewsChange);
    setChangeIndicator("statEngagementsChange", engChange);
    setChangeIndicator("statRateChange", rateChange, true);
  }
  function setChangeIndicator(elementId, change, isPercent = false) {
    const el = document.getElementById(elementId);
    if (!el) return;
    if (change === 0) {
      el.textContent = "--";
      el.className = "stat-change";
    } else {
      const prefix = change > 0 ? "+" : "";
      el.textContent = prefix + (isPercent ? change.toFixed(1) + "%" : formatNumber(change));
      el.className = "stat-change " + (change > 0 ? "positive" : "negative");
    }
  }
  function formatNumber(num) {
    if (num >= 1e6) return (num / 1e6).toFixed(1) + "M";
    if (num >= 1e3) return (num / 1e3).toFixed(1) + "K";
    return num.toString();
  }
  function renderActivityChart(dailyData) {
    const barsContainer = document.getElementById("chartBars");
    const labelsContainer = document.getElementById("chartLabels");
    if (!barsContainer || !labelsContainer) return;
    if (dailyData.length === 0) {
      barsContainer.innerHTML = '<div class="empty-state-inline"><p>No data available</p></div>';
      labelsContainer.innerHTML = "";
      return;
    }
    const maxPosts = Math.max(...dailyData.map((d) => d.posts || 0), 1);
    const maxViews = Math.max(...dailyData.map((d) => d.views || 0), 1);
    barsContainer.innerHTML = dailyData.slice(-14).map((d) => {
      const postsHeight = (d.posts || 0) / maxPosts * 100;
      const viewsHeight = (d.views || 0) / maxViews * 50;
      const engHeight = (d.engagements || 0) / Math.max(d.views, 1) * 100;
      return `
      <div class="chart-bar-group" title="${d.date}: ${d.posts} posts, ${d.views} views">
        <div class="chart-bar posts" style="height: ${postsHeight}%"></div>
      </div>
    `;
    }).join("");
    labelsContainer.innerHTML = dailyData.slice(-14).map((d, i) => {
      if (i % 2 === 0 || dailyData.length <= 7) {
        const date = new Date(d.date);
        return `<span class="chart-label">${date.getDate()}/${date.getMonth() + 1}</span>`;
      }
      return '<span class="chart-label"></span>';
    }).join("");
  }
  function renderPlatformPerformance(platformStats) {
    const container = document.getElementById("platformStats");
    if (!container) return;
    const sortedPlatforms = Object.entries(platformStats).filter(([_, value]) => value > 0).sort((a, b) => b[1] - a[1]).slice(0, 10);
    if (sortedPlatforms.length === 0) {
      container.innerHTML = '<div class="empty-state">No platform data yet</div>';
      return;
    }
    const maxValue = Math.max(...sortedPlatforms.map(([_, v]) => v), 1);
    container.innerHTML = sortedPlatforms.map(([key, value]) => {
      const percent = value / maxValue * 100;
      const icon = PLATFORM_ICONS[key] || "\u{1F4C4}";
      const name = key.charAt(0).toUpperCase() + key.slice(1);
      return `
      <div class="platform-perf-item">
        <div class="platform-perf-info">
          <span class="platform-perf-icon">${icon}</span>
          <span class="platform-perf-name">${name}</span>
        </div>
        <div class="platform-perf-bar-container">
          <div class="platform-perf-bar" style="width: ${percent}%"></div>
        </div>
        <span class="platform-perf-value">${formatNumber(value)}</span>
      </div>
    `;
    }).join("");
  }
  function renderContentBreakdown(breakdown) {
    const total = (breakdown.articles || 0) + (breakdown.replies || 0) + (breakdown.reposts || 0) + (breakdown.comments || 0);
    if (total === 0) return;
    const calcPercent = (val) => val / total * 100;
    document.getElementById("breakdownArticles").textContent = (breakdown.articles || 0).toString();
    document.getElementById("breakdownReplies").textContent = (breakdown.replies || 0).toString();
    document.getElementById("breakdownReposts").textContent = (breakdown.reposts || 0).toString();
    document.getElementById("breakdownComments").textContent = (breakdown.comments || 0).toString();
    document.getElementById("breakdownArticlesBar").style.width = calcPercent(breakdown.articles || 0) + "%";
    document.getElementById("breakdownRepliesBar").style.width = calcPercent(breakdown.replies || 0) + "%";
    document.getElementById("breakdownRepostsBar").style.width = calcPercent(breakdown.reposts || 0) + "%";
    document.getElementById("breakdownCommentsBar").style.width = calcPercent(breakdown.comments || 0) + "%";
  }
  function renderBestContent(bestContent) {
    const container = document.getElementById("bestContentList");
    if (!container) return;
    if (bestContent.length === 0) {
      container.innerHTML = `
      <div class="empty-state-inline">
        <p>No content published yet. Start posting to see your top performers!</p>
      </div>
    `;
      return;
    }
    const rankClasses = ["gold", "silver", "bronze"];
    container.innerHTML = bestContent.slice(0, 5).map((item, i) => `
    <div class="best-content-item">
      <div class="best-content-rank ${rankClasses[i] || ""}">${i + 1}</div>
      <div class="best-content-info">
        <div class="best-content-title">${escapeHtml(item.title || item.content?.substring(0, 50) || "Untitled")}</div>
        <div class="best-content-meta">
          <span>${PLATFORM_ICONS[item.platform] || "\u{1F4C4}"} ${item.platform}</span>
          <span>${formatDate(item.published_at)}</span>
        </div>
      </div>
      <div class="best-content-stats">
        <div class="best-content-stat">
          <span class="best-content-stat-value">${formatNumber(item.views || 0)}</span>
          <span class="best-content-stat-label">views</span>
        </div>
        <div class="best-content-stat">
          <span class="best-content-stat-value">${formatNumber(item.engagements || 0)}</span>
          <span class="best-content-stat-label">engagements</span>
        </div>
      </div>
    </div>
  `).join("");
  }
  function renderActivityHeatmap(heatmapData) {
    const container = document.getElementById("heatmapGrid");
    if (!container) return;
    const cells = [];
    const today = /* @__PURE__ */ new Date();
    for (let i = 89; i >= 0; i--) {
      const date = new Date(today);
      date.setDate(date.getDate() - i);
      const dateStr = date.toISOString().split("T")[0];
      const dayData = heatmapData.find((d) => d.date === dateStr);
      const count = dayData?.count || 0;
      let level = "level-0";
      if (count > 0) level = "level-1";
      if (count >= 3) level = "level-2";
      if (count >= 5) level = "level-3";
      if (count >= 10) level = "level-4";
      cells.push(`<div class="heatmap-cell ${level}" title="${dateStr}: ${count} posts"></div>`);
    }
    container.innerHTML = cells.join("");
  }
  function setDefaultStatsUI() {
    document.getElementById("statPosts").textContent = "0";
    document.getElementById("statViews").textContent = "0";
    document.getElementById("statEngagements").textContent = "0";
    document.getElementById("statRate").textContent = "0%";
    ["statPostsChange", "statViewsChange", "statEngagementsChange", "statRateChange"].forEach((id) => {
      const el = document.getElementById(id);
      if (el) {
        el.textContent = "--";
        el.className = "stat-change";
      }
    });
  }
  function initStatsEvents() {
    const timeRange = document.getElementById("statsTimeRange");
    if (timeRange && !timeRange.hasAttribute("data-listener")) {
      timeRange.addEventListener("change", () => loadStats());
      timeRange.setAttribute("data-listener", "true");
    }
    const exportBtn = document.getElementById("btnExportStats");
    if (exportBtn && !exportBtn.hasAttribute("data-listener")) {
      exportBtn.addEventListener("click", exportStats);
      exportBtn.setAttribute("data-listener", "true");
    }
  }
  async function exportStats() {
    try {
      const days = parseInt(document.getElementById("statsTimeRange")?.value || "30");
      const stats = await invoke2("get_detailed_stats", { days });
      const csvContent = generateStatsCSV(stats);
      downloadCSV(csvContent, `unmarket_stats_${(/* @__PURE__ */ new Date()).toISOString().split("T")[0]}.csv`);
      showToast("Statistics exported successfully!", "success");
    } catch (error) {
      showToast("Failed to export statistics", "error");
    }
  }
  function generateStatsCSV(stats) {
    let csv = "Date,Posts,Views,Engagements\n";
    (stats.daily_data || []).forEach((d) => {
      csv += `${d.date},${d.posts || 0},${d.views || 0},${d.engagements || 0}
`;
    });
    return csv;
  }
  function downloadCSV(content, filename) {
    const blob = new Blob([content], { type: "text/csv;charset=utf-8;" });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = filename;
    link.click();
  }
  async function loadSettings() {
    populateDefaultModels();
    const langSelector = document.getElementById("languageSelect");
    if (langSelector) {
      langSelector.value = currentLanguage;
    }
    try {
      try {
        const providers = await invoke2("get_ai_providers");
        aiProviders = { ...defaultAiProviders, ...providers || {} };
        populateDefaultModels();
      } catch {
      }
      const config = await invoke2("get_config");
      try {
        const unzooPath = await invoke2("detect_unzoo_path");
        document.getElementById("unzooPath").value = unzooPath;
      } catch {
        document.getElementById("unzooPath").value = t("settings.notFound");
      }
      if (config.scheduler) {
        document.getElementById("schedulerMode").value = config.scheduler.mode || "round-robin";
        document.getElementById("schedulerInterval").value = config.scheduler.interval_minutes?.toString() || "60";
        document.getElementById("schedulerMaxPosts").value = config.scheduler.max_daily_posts?.toString() || "50";
      }
      await loadAIConfig();
      await loadScheduledJobs();
      await loadProxies();
      await loadSettingsProfiles();
      setupProfileHandlers();
    } catch (error) {
      console.error("Settings error:", error);
    }
  }
  async function loadSettingsProfiles() {
    const select = document.getElementById("browserProfile");
    const statusDiv = document.getElementById("profileStatus");
    const detailDiv = document.getElementById("browserStatusDetail");
    if (!select) return;
    try {
      const profiles = await invoke2("get_available_browser_profiles");
      const selectedProfile = await invoke2("get_selected_browser_profile");
      select.innerHTML = '<option value="">-- Select Profile --</option>';
      profiles.forEach((p) => {
        const option = document.createElement("option");
        option.value = p.id;
        option.textContent = `${p.name} (${p.id})`;
        if (p.id === selectedProfile) {
          option.selected = true;
        }
        select.appendChild(option);
      });
      if (selectedProfile && statusDiv) {
        statusDiv.innerHTML = `<span style="color: var(--success);">\u2713 \u5DF2\u9009\u62E9: ${selectedProfile}</span>`;
      }
      const status = await invoke2("get_browser_status");
      if (detailDiv) {
        detailDiv.innerHTML = status.connected ? `<span class="status-badge" style="background: var(--success); color: #fff;">\u2713 Unzoo \u5DF2\u8FDE\u63A5${status.active_tab ? " \xB7 Tab " + escapeHtml(String(status.active_tab)) : ""}</span>` : `<span class="status-badge" style="background: var(--danger); color: #fff;">\u2715 Unzoo \u672A\u8FDE\u63A5</span>`;
      }
      if (status.connected) {
        if (statusDiv) {
          statusDiv.innerHTML = `<span style="color: var(--success);">\u2713 \u5DF2\u8FDE\u63A5 (Tab: ${status.active_tab || "Unknown"})</span>`;
        }
      }
    } catch (error) {
      console.error("Failed to load browser profiles:", error);
      if (detailDiv) {
        detailDiv.innerHTML = `<span class="status-badge" style="background: var(--danger); color: #fff;">\u2715 \u65E0\u6CD5\u8FDE\u63A5 Unzoo</span>`;
      }
      if (statusDiv) {
        statusDiv.innerHTML = `<span style="color: var(--danger);">\u26A0 \u65E0\u6CD5\u52A0\u8F7D Profiles: ${error}</span>`;
      }
    }
  }
  function setupProfileHandlers() {
    document.getElementById("btnRefreshProfiles")?.addEventListener("click", async () => {
      await loadSettingsProfiles();
      showToast("Profiles refreshed", "success");
    });
    document.getElementById("btnConnectProfile")?.addEventListener("click", async () => {
      const select = document.getElementById("browserProfile");
      const statusDiv = document.getElementById("profileStatus");
      const profileId = select?.value;
      if (!profileId) {
        showToast(t("msg.selectProfileFirst"), "error");
        return;
      }
      if (statusDiv) {
        statusDiv.innerHTML = '<span style="color: var(--warning);">\u6B63\u5728\u8FDE\u63A5...</span>';
      }
      try {
        const result = await invoke2("connect_browser_profile", { profileId });
        if (result.success) {
          showToast(`\u5DF2\u8FDE\u63A5\u5230 ${profileId}`, "success");
          if (statusDiv) {
            statusDiv.innerHTML = `<span style="color: var(--success);">\u2713 \u5DF2\u8FDE\u63A5: ${profileId} (Tab: ${result.tab_id})</span>`;
          }
        }
      } catch (error) {
        console.error("Failed to connect profile:", error);
        showToast(`\u8FDE\u63A5\u5931\u8D25: ${error}`, "error");
        if (statusDiv) {
          statusDiv.innerHTML = `<span style="color: var(--danger);">\u26A0 \u8FDE\u63A5\u5931\u8D25: ${error}</span>`;
        }
      }
    });
  }
  var proxies = [];
  async function loadProxies() {
    try {
      const data = await invoke2("list_proxies");
      proxies = data.proxies || [];
      updateProxyStats(data.stats || {});
      renderProxies();
    } catch (error) {
      console.error("Failed to load proxies:", error);
      proxies = [];
      renderProxies();
    }
  }
  function updateProxyStats(stats) {
    document.getElementById("proxyStatTotal").textContent = stats.total?.toString() || "0";
    document.getElementById("proxyStatActive").textContent = stats.active?.toString() || "0";
    document.getElementById("proxyStatUsed").textContent = stats.in_use?.toString() || "0";
    document.getElementById("proxyStatFailed").textContent = stats.failed?.toString() || "0";
  }
  function renderProxies() {
    const container = document.getElementById("proxyList");
    if (!container) return;
    if (proxies.length === 0) {
      container.innerHTML = `
      <div class="empty-state-inline">
        <p>No proxies configured. Add a proxy to get started.</p>
      </div>
    `;
      return;
    }
    container.innerHTML = proxies.map((p) => `
    <div class="proxy-item" data-id="${p.id}">
      <div class="proxy-info">
        <span class="proxy-name">${escapeHtml(p.name || "Unnamed")}</span>
        <span class="proxy-address">${p.protocol}://${p.host}:${p.port}</span>
      </div>
      <div class="proxy-meta">
        <span class="proxy-status ${p.status}">${p.status}</span>
        ${p.in_use ? '<span class="proxy-tag">In Use</span>' : ""}
        ${(p.tags || []).map((t2) => `<span class="proxy-tag">${escapeHtml(t2)}</span>`).join("")}
      </div>
      <div class="proxy-actions">
        <button class="btn btn-small btn-secondary" onclick="testProxy('${p.id}')" title="Test Proxy">\u{1F50D}</button>
        <button class="btn btn-small btn-danger" onclick="deleteProxy('${p.id}')" title="Delete">\u{1F5D1}\uFE0F</button>
      </div>
    </div>
  `).join("");
  }
  function showAddProxyModal() {
    document.getElementById("proxyName").value = "";
    document.getElementById("proxyProtocol").value = "socks5";
    document.getElementById("proxyHost").value = "";
    document.getElementById("proxyPort").value = "";
    document.getElementById("proxyUsername").value = "";
    document.getElementById("proxyPassword").value = "";
    document.getElementById("proxyTags").value = "";
    openModal("modalAddProxy");
  }
  window.showAddProxyModal = showAddProxyModal;
  async function savePoolProxy() {
    const name = document.getElementById("proxyName")?.value?.trim();
    const protocol = document.getElementById("proxyProtocol")?.value;
    const host = document.getElementById("proxyHost")?.value?.trim();
    const port = parseInt(document.getElementById("proxyPort")?.value) || 0;
    const username = document.getElementById("proxyUsername")?.value?.trim();
    const password = document.getElementById("proxyPassword")?.value?.trim();
    const tagsStr = document.getElementById("proxyTags")?.value?.trim();
    const tags = tagsStr ? tagsStr.split(",").map((t2) => t2.trim()).filter((t2) => t2) : [];
    if (!host || !port) {
      showToast("Please enter host and port", "error");
      return;
    }
    try {
      await invoke2("add_proxy", { name, protocol, host, port, username, password, tags });
      closeModal("modalAddProxy");
      showToast(t("msg.proxyAdded"), "success");
      await loadProxies();
    } catch (error) {
      console.error("Failed to add proxy:", error);
      showToast("Failed to add proxy: " + error, "error");
    }
  }
  async function bulkImportProxies() {
    const input = document.getElementById("bulkProxyInput")?.value?.trim();
    if (!input) {
      showToast("Please enter proxy URLs", "error");
      return;
    }
    const lines = input.split("\n").filter((l) => l.trim());
    let added = 0;
    let failed = 0;
    for (const line of lines) {
      try {
        const match = line.match(/^(https?|socks5?):\/\/(?:([^:]+):([^@]+)@)?([^:]+):(\d+)$/i);
        if (!match) {
          failed++;
          continue;
        }
        const [, protocol, username, password, host, port] = match;
        await invoke2("add_proxy", {
          name: `${host}:${port}`,
          protocol: protocol.toLowerCase(),
          host,
          port: parseInt(port),
          username: username || null,
          password: password || null,
          tags: []
        });
        added++;
      } catch {
        failed++;
      }
    }
    document.getElementById("bulkProxyInput").value = "";
    showToast(`Imported ${added} proxies${failed > 0 ? `, ${failed} failed` : ""}`, added > 0 ? "success" : "error");
    await loadProxies();
  }
  async function testProxy(id) {
    try {
      showToast("Testing proxy...", "info");
      const result = await invoke2("test_proxy", { id });
      if (result.success) {
        showToast(`Proxy working! Latency: ${result.latency_ms}ms`, "success");
      } else {
        showToast(`Proxy failed: ${result.error}`, "error");
      }
      await loadProxies();
    } catch (error) {
      console.error("Failed to test proxy:", error);
      showToast("Failed to test proxy: " + error, "error");
    }
  }
  window.testProxy = testProxy;
  async function deleteProxy(id) {
    if (!confirm("Delete this proxy?")) return;
    try {
      await invoke2("delete_proxy", { id });
      showToast("Proxy deleted", "success");
      await loadProxies();
    } catch (error) {
      console.error("Failed to delete proxy:", error);
      showToast("Failed to delete proxy: " + error, "error");
    }
  }
  window.deleteProxy = deleteProxy;
  function initProxyEvents() {
    document.getElementById("btnAddProxy")?.addEventListener("click", showAddProxyModal);
    document.getElementById("btnSavePoolProxy")?.addEventListener("click", savePoolProxy);
    document.getElementById("btnBulkImportProxy")?.addEventListener("click", bulkImportProxies);
  }
  function populateDefaultModels() {
    const provider = document.getElementById("aiProvider")?.value;
    const modelSelect = document.getElementById("aiModel");
    if (!modelSelect || !aiProviders[provider]) return;
    modelSelect.innerHTML = aiProviders[provider].models.map((m) => `<option value="${m}">${m}</option>`).join("");
  }
  function updateAIKeyVisibility() {
    const provider = document.getElementById("aiProvider")?.value;
    document.querySelectorAll(".ai-key-field").forEach((el) => {
      el.style.display = el.dataset.provider === provider ? "" : "none";
    });
  }
  async function refreshModels() {
    const provider = document.getElementById("aiProvider")?.value;
    const btn = document.getElementById("btnRefreshModels");
    const modelSelect = document.getElementById("aiModel");
    const geminiKey = document.getElementById("aiKeyGemini")?.value;
    const openaiKey = document.getElementById("aiKeyOpenai")?.value;
    const deepseekKey = document.getElementById("aiKeyDeepseek")?.value;
    const qwenKey = document.getElementById("aiKeyQwen")?.value;
    const currentKey = provider === "gemini" ? geminiKey : provider === "openai" ? openaiKey : provider === "deepseek" ? deepseekKey : provider === "qwen" ? qwenKey : "";
    if (!currentKey || currentKey.trim() === "") {
      showToast(`\u8BF7\u5148\u8F93\u5165 ${aiProviders[provider]?.name || provider} \u7684 API Key`, "warning");
      return;
    }
    try {
      if (btn) {
        btn.disabled = true;
        btn.textContent = "\u83B7\u53D6\u4E2D...";
      }
      await invoke2("configure_ai", {
        provider,
        model: modelSelect?.value || "",
        geminiKey: geminiKey || null,
        openaiKey: openaiKey || null,
        deepseekKey: deepseekKey || null,
        qwenKey: qwenKey || null
      });
      const models = await invoke2("fetch_available_models", { provider });
      if (models && models.length > 0) {
        modelSelect.innerHTML = models.map((m) => `<option value="${m}">${m}</option>`).join("");
        showToast(`\u83B7\u53D6\u5230 ${models.length} \u4E2A\u53EF\u7528\u6A21\u578B`, "success");
      } else {
        populateDefaultModels();
        showToast(t("msg.usingDefaultModels"), "info");
      }
    } catch (error) {
      console.error("Failed to refresh models:", error);
      populateDefaultModels();
      const errMsg = error?.toString() || "";
      if (errMsg.includes("No API key")) {
        showToast(t("msg.apiKeyNotSaved"), "error");
      } else if (errMsg.includes("Failed to fetch")) {
        showToast(`${t("msg.networkError")}, ${t("msg.usingDefaultModels")}`, "warning");
      } else {
        showToast(`${t("msg.failed")}, ${t("msg.usingDefaultModels")}`, "warning");
      }
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = `\u{1F504} ${t("settings.refreshModels")}`;
      }
    }
  }
  async function saveAISettings() {
    const provider = document.getElementById("aiProvider")?.value;
    const model = document.getElementById("aiModel")?.value;
    const geminiKey = document.getElementById("aiKeyGemini")?.value;
    const openaiKey = document.getElementById("aiKeyOpenai")?.value;
    const deepseekKey = document.getElementById("aiKeyDeepseek")?.value;
    const qwenKey = document.getElementById("aiKeyQwen")?.value;
    try {
      await invoke2("configure_ai", {
        provider,
        model,
        geminiKey: geminiKey || null,
        openaiKey: openaiKey || null,
        deepseekKey: deepseekKey || null,
        qwenKey: qwenKey || null
      });
      showToast(t("msg.aiSettingsSaved"), "success");
    } catch (error) {
      console.error("Failed to save AI settings:", error);
      showToast("Failed to save AI settings", "error");
    }
  }
  async function testAIConnection() {
    const provider = document.getElementById("aiProvider")?.value;
    const model = document.getElementById("aiModel")?.value || "";
    const keyFieldMap = {
      gemini: "aiKeyGemini",
      openai: "aiKeyOpenai",
      deepseek: "aiKeyDeepseek",
      qwen: "aiKeyQwen"
    };
    const apiKey = document.getElementById(keyFieldMap[provider])?.value || "";
    const btn = document.getElementById("btnTestAI");
    const statusEl = document.getElementById("aiSaveStatus");
    if (!apiKey.trim()) {
      showToast(`\u8BF7\u5148\u8F93\u5165 ${aiProviders[provider]?.name || provider} \u7684 API Key`, "warning");
      return;
    }
    const origText = btn?.textContent || "Test Connection";
    try {
      if (btn) {
        btn.disabled = true;
        btn.textContent = "\u6D4B\u8BD5\u4E2D\u2026";
      }
      if (statusEl) {
        statusEl.textContent = "\u23F3 \u6B63\u5728\u6D4B\u8BD5\u8FDE\u63A5\u2026";
        statusEl.style.color = "";
      }
      const msg = await invoke2("test_ai_connection", { provider, key: apiKey, model: model || null });
      if (statusEl) {
        statusEl.textContent = msg;
        statusEl.style.color = "#1a9d4a";
      }
      showToast(msg, "success");
    } catch (error) {
      const em = error?.toString() || "\u8FDE\u63A5\u5931\u8D25";
      if (statusEl) {
        statusEl.textContent = "\u2717 " + em;
        statusEl.style.color = "#e55";
      }
      showToast("\u8FDE\u63A5\u5931\u8D25\uFF1A" + em, "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = origText;
      }
    }
  }
  async function loadAIConfig() {
    try {
      const config = await invoke2("get_ai_config");
      if (config.provider) {
        const providerSelect = document.getElementById("aiProvider");
        if (providerSelect) providerSelect.value = config.provider;
        populateDefaultModels();
      }
      updateAIKeyVisibility();
      if (config.model) {
        const modelSelect = document.getElementById("aiModel");
        if (modelSelect) modelSelect.value = config.model;
      }
      if (config.gemini_key) {
        document.getElementById("aiKeyGemini").value = config.gemini_key;
      }
      if (config.openai_key) {
        document.getElementById("aiKeyOpenai").value = config.openai_key;
      }
      if (config.deepseek_key) {
        document.getElementById("aiKeyDeepseek").value = config.deepseek_key;
      }
      if (config.qwen_key) {
        document.getElementById("aiKeyQwen").value = config.qwen_key;
      }
    } catch (error) {
      console.error("Failed to load AI config:", error);
    }
  }
  async function saveSchedulerSettings() {
    const mode = document.getElementById("schedulerMode")?.value;
    const interval = document.getElementById("schedulerInterval")?.value;
    const maxPosts = document.getElementById("schedulerMaxPosts")?.value;
    try {
      await invoke2("set_config", { key: "scheduler.mode", value: mode });
      await invoke2("set_config", { key: "scheduler.interval_minutes", value: interval });
      await invoke2("set_config", { key: "scheduler.max_daily_posts", value: maxPosts });
      showToast("Scheduler settings saved", "success");
    } catch (error) {
      showToast("Failed to save scheduler settings", "error");
    }
  }
  async function checkBrowserStatus() {
    try {
      const available = await invoke2("check_browser_status");
      const el = document.getElementById("browserStatus");
      if (el) {
        el.innerHTML = available ? '<span class="status-dot online"></span><span class="status-text">Unzoo Ready</span>' : '<span class="status-dot offline"></span><span class="status-text">Unzoo Not Found</span>';
      }
    } catch (error) {
      console.error("Browser status error:", error);
    }
  }
  function escapeHtml(str) {
    if (!str) return "";
    return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }
  function showToast(message, type = "info") {
    const container = document.getElementById("toastContainer");
    if (!container) return;
    const toast = document.createElement("div");
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    container.appendChild(toast);
    setTimeout(() => {
      toast.classList.add("fade-out");
      setTimeout(() => toast.remove(), 300);
    }, 3e3);
  }
  function loadArticlesPage() {
    const select = document.getElementById("articleProduct");
    if (select) {
      select.innerHTML = '<option value="">Choose a product...</option>' + products.map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
    }
    renderSavedArticles();
  }
  function initArticleTypeSelection() {
    document.querySelectorAll(".radio-card").forEach((card) => {
      card.addEventListener("click", () => {
        document.querySelectorAll(".radio-card").forEach((c) => c.classList.remove("active"));
        card.classList.add("active");
        const input = card.querySelector('input[type="radio"]');
        if (input) input.checked = true;
      });
    });
  }
  async function generateArticles() {
    const productId = document.getElementById("articleProduct")?.value;
    if (!productId) {
      showToast("Please select a product", "error");
      return;
    }
    const product = products.find((p) => p.id === productId);
    if (!product) return;
    const articleType = document.querySelector('input[name="articleType"]:checked')?.value || "tutorial";
    const platforms = Array.from(document.querySelectorAll("#articlePlatforms input:checked")).map((cb) => cb.value);
    const languages = Array.from(document.querySelectorAll("#articleLanguages input:checked")).map((cb) => cb.value);
    const keywords2 = document.getElementById("articleKeywords")?.value?.split(",").map((k) => k.trim()).filter((k) => k) || [];
    const tone = document.getElementById("articleTone")?.value || "casual";
    if (platforms.length === 0 || languages.length === 0) {
      showToast("Select at least one platform and language", "error");
      return;
    }
    const btn = document.getElementById("btnGenerateArticle");
    if (btn) {
      btn.disabled = true;
      btn.textContent = "Generating...";
    }
    try {
      generatedArticles = [];
      const total = platforms.length * languages.length;
      let current = 0;
      for (const platform of platforms) {
        for (const language of languages) {
          current++;
          if (btn) btn.textContent = `Generating (${current}/${total})...`;
          const article = await generateSingleArticle(product, articleType, platform, language, keywords2, tone);
          generatedArticles.push(article);
        }
      }
      currentArticleIndex = 0;
      renderArticleVersions();
      renderCurrentArticle();
      document.getElementById("btnPublishArticle").disabled = false;
      showToast(`Generated ${generatedArticles.length} articles`, "success");
    } catch (error) {
      console.error("Failed to generate articles:", error);
      showToast("Failed to generate articles", "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = "Generate Articles";
      }
    }
  }
  async function generateSingleArticle(product, type, platform, language, keywords2, tone) {
    const lengthTargets = {
      zhihu: 1500,
      wechat: 2e3,
      medium: 1500,
      devto: 1200,
      toutiao: 1e3,
      reddit: 500,
      blog: 1800
    };
    const targetLength = lengthTargets[platform] || 1e3;
    try {
      const results = await invoke2("generate_article", {
        productId: product.id,
        articleType: type,
        platforms: [platform],
        languages: [language],
        keywords: keywords2,
        tone
      });
      if (results && results.length > 0) {
        const result = results[0];
        return {
          id: result.id || generateTaskId(),
          productId: product.id,
          productName: product.name,
          type,
          platform,
          language,
          title: result.title,
          body: result.body,
          keywords: result.keywords || keywords2,
          wordCount: result.word_count || result.body.split(/\s+/).length,
          createdAt: /* @__PURE__ */ new Date()
        };
      }
      throw new Error("No article generated");
    } catch (error) {
      console.warn("AI generation failed, using template:", error);
      return generateArticleFromTemplate(product, type, platform, language, keywords2, targetLength);
    }
  }
  function generateArticleFromTemplate(product, type, platform, language, keywords2, targetLength) {
    const isZh = language === "zh";
    const templates = getArticleTemplates(isZh);
    const template = templates[type] || templates.tutorial;
    let title = template.title.replace("{product}", product.name).replace("{tagline}", product.tagline || "");
    let body = template.body.replace(/{product}/g, product.name).replace(/{tagline}/g, product.tagline || "").replace(/{description}/g, product.description || product.tagline || "").replace(/{url}/g, product.url).replace(/{keywords}/g, keywords2.join(", "));
    if (platform === "zhihu") {
      body = body + (isZh ? "\n\n---\n\n\u5E0C\u671B\u8FD9\u7BC7\u6587\u7AE0\u5BF9\u4F60\u6709\u5E2E\u52A9\uFF01\u6B22\u8FCE\u70B9\u8D5E\u3001\u6536\u85CF\u548C\u5173\u6CE8\u3002" : "\n\n---\n\nHope this helps! Feel free to upvote if you found it useful.");
    } else if (platform === "medium") {
      body = body + "\n\n---\n\n*If you enjoyed this article, please clap and follow for more content!*";
    } else if (platform === "wechat") {
      body = body + "\n\n---\n\n\u89C9\u5F97\u6709\u7528\u7684\u8BDD\uFF0C\u6B22\u8FCE\u70B9\u8D5E\u5728\u770B\uFF0C\u5206\u4EAB\u7ED9\u66F4\u591A\u670B\u53CB\uFF01";
    }
    return {
      id: generateTaskId(),
      productId: product.id,
      productName: product.name,
      type,
      platform,
      language,
      title,
      body,
      keywords: keywords2,
      wordCount: body.length,
      createdAt: /* @__PURE__ */ new Date()
    };
  }
  function getArticleTemplates(isZh) {
    if (isZh) {
      return {
        tutorial: {
          title: "{product} \u4F7F\u7528\u6307\u5357\uFF1A\u4ECE\u5165\u95E8\u5230\u7CBE\u901A",
          body: `# {product} \u4F7F\u7528\u6307\u5357

## \u524D\u8A00

{tagline}

\u5728\u8FD9\u7BC7\u6587\u7AE0\u4E2D\uFF0C\u6211\u5C06\u8BE6\u7EC6\u4ECB\u7ECD {product} \u7684\u6838\u5FC3\u529F\u80FD\u548C\u4F7F\u7528\u6280\u5DE7\uFF0C\u5E2E\u52A9\u4F60\u5FEB\u901F\u4E0A\u624B\u3002

## \u4EC0\u4E48\u662F {product}\uFF1F

{description}

## \u6838\u5FC3\u529F\u80FD

### 1. \u4E3B\u8981\u7279\u6027

{product} \u63D0\u4F9B\u4E86\u4EE5\u4E0B\u5F3A\u5927\u529F\u80FD\uFF1A

- **\u7B80\u6D01\u6613\u7528**\uFF1A\u754C\u9762\u76F4\u89C2\uFF0C\u4E0A\u624B\u7B80\u5355
- **\u529F\u80FD\u5F3A\u5927**\uFF1A\u6EE1\u8DB3\u4E13\u4E1A\u9700\u6C42
- **\u6301\u7EED\u66F4\u65B0**\uFF1A\u56E2\u961F\u6D3B\u8DC3\uFF0C\u529F\u80FD\u4E0D\u65AD\u5B8C\u5584

### 2. \u4F7F\u7528\u573A\u666F

\u65E0\u8BBA\u4F60\u662F\u4E2A\u4EBA\u7528\u6237\u8FD8\u662F\u56E2\u961F\uFF0C{product} \u90FD\u80FD\u6EE1\u8DB3\u4F60\u7684\u9700\u6C42\uFF1A

- \u65E5\u5E38\u5DE5\u4F5C
- \u9879\u76EE\u7BA1\u7406
- \u56E2\u961F\u534F\u4F5C

## \u5FEB\u901F\u5F00\u59CB

1. \u8BBF\u95EE {url}
2. \u6CE8\u518C\u8D26\u53F7
3. \u5F00\u59CB\u4F7F\u7528

## \u8FDB\u9636\u6280\u5DE7

\u638C\u63E1\u4EE5\u4E0B\u6280\u5DE7\uFF0C\u8BA9\u4F60\u7684\u6548\u7387\u7FFB\u500D\uFF1A

1. \u5584\u7528\u5FEB\u6377\u952E
2. \u81EA\u5B9A\u4E49\u8BBE\u7F6E
3. \u63A2\u7D22\u9AD8\u7EA7\u529F\u80FD

## \u603B\u7ED3

{product} \u662F\u4E00\u6B3E\u503C\u5F97\u5C1D\u8BD5\u7684\u5DE5\u5177\u3002{tagline}

\u7ACB\u5373\u4F53\u9A8C\uFF1A{url}`
        },
        comparison: {
          title: "{product} vs \u7ADE\u54C1\u5BF9\u6BD4\uFF1A\u4E3A\u4EC0\u4E48\u6211\u9009\u62E9 {product}",
          body: `# {product} vs \u7ADE\u54C1\u5BF9\u6BD4

## \u6211\u7684\u9009\u62E9\u56F0\u96BE

\u5728\u4F17\u591A\u5DE5\u5177\u4E2D\u9009\u62E9\u5408\u9002\u7684\u4EA7\u54C1\u5E76\u4E0D\u5BB9\u6613\u3002\u4ECA\u5929\uFF0C\u6211\u60F3\u5206\u4EAB\u4E00\u4E0B\u4E3A\u4EC0\u4E48\u6211\u6700\u7EC8\u9009\u62E9\u4E86 {product}\u3002

## \u4EA7\u54C1\u4ECB\u7ECD

{description}

## \u5BF9\u6BD4\u7EF4\u5EA6

### \u6613\u7528\u6027

{product} \u7684\u754C\u9762\u8BBE\u8BA1\u7B80\u6D01\u76F4\u89C2\uFF0C\u5B66\u4E60\u6210\u672C\u6781\u4F4E\u3002

### \u529F\u80FD\u6027

\u6838\u5FC3\u529F\u80FD\u5B8C\u5907\uFF0C\u6EE1\u8DB3\u65E5\u5E38\u9700\u6C42\u3002

### \u6027\u4EF7\u6BD4

\u76F8\u6BD4\u540C\u7C7B\u4EA7\u54C1\uFF0C{product} \u63D0\u4F9B\u4E86\u66F4\u597D\u7684\u4EF7\u503C\u3002

## \u4E3A\u4EC0\u4E48\u9009\u62E9 {product}

1. **{tagline}**
2. \u6301\u7EED\u7684\u4EA7\u54C1\u8FED\u4EE3
3. \u6D3B\u8DC3\u7684\u793E\u533A\u652F\u6301

## \u7ED3\u8BBA

\u5982\u679C\u4F60\u6B63\u5728\u5BFB\u627E\u4E00\u6B3E\u53EF\u9760\u7684\u5DE5\u5177\uFF0C{product} \u503C\u5F97\u4E00\u8BD5\u3002

\u4E86\u89E3\u66F4\u591A\uFF1A{url}`
        },
        problem: {
          title: "\u89E3\u51B3 {keywords} \u95EE\u9898\uFF0C\u6211\u627E\u5230\u4E86 {product}",
          body: `# \u5982\u4F55\u89E3\u51B3 {keywords} \u95EE\u9898

## \u56F0\u6270

\u76F8\u4FE1\u5F88\u591A\u4EBA\u90FD\u9047\u5230\u8FC7\u8FD9\u6837\u7684\u95EE\u9898\uFF1A\u6548\u7387\u4F4E\u4E0B\u3001\u5DE5\u5177\u4E0D\u987A\u624B\u3001\u5DE5\u4F5C\u6D41\u7A0B\u7E41\u7410\u3002

\u6211\u4E5F\u66FE\u6DF1\u53D7\u5176\u6270\uFF0C\u76F4\u5230\u6211\u53D1\u73B0\u4E86 {product}\u3002

## \u89E3\u51B3\u65B9\u6848

{product} - {tagline}

{description}

## \u5B83\u662F\u5982\u4F55\u5E2E\u52A9\u6211\u7684

### \u95EE\u98981\uFF1A\u6548\u7387\u95EE\u9898
{product} \u901A\u8FC7\u667A\u80FD\u5316\u529F\u80FD\u5927\u5E45\u63D0\u5347\u4E86\u6211\u7684\u5DE5\u4F5C\u6548\u7387\u3002

### \u95EE\u98982\uFF1A\u6613\u7528\u6027
\u7B80\u6D01\u7684\u754C\u9762\u8BBE\u8BA1\u8BA9\u6211\u80FD\u5FEB\u901F\u4E0A\u624B\u3002

### \u95EE\u98983\uFF1A\u534F\u4F5C\u9700\u6C42
\u5B8C\u5584\u7684\u534F\u4F5C\u529F\u80FD\u6EE1\u8DB3\u4E86\u56E2\u961F\u9700\u6C42\u3002

## \u5B9E\u9645\u6548\u679C

\u4F7F\u7528 {product} \u540E\uFF0C\u6211\u7684\u6548\u7387\u63D0\u5347\u4E86\u81F3\u5C11 30%\u3002

## \u5982\u4F55\u5F00\u59CB

\u8BBF\u95EE {url} \u7ACB\u5373\u4F53\u9A8C\u3002

\u4F60\u4E5F\u6709\u7C7B\u4F3C\u7684\u56F0\u6270\u5417\uFF1F\u8BD5\u8BD5 {product} \u5427\uFF01`
        },
        story: {
          title: "\u6211\u4E0E {product} \u7684\u6545\u4E8B",
          body: `# \u6211\u4E0E {product} \u7684\u6545\u4E8B

## \u9047\u89C1

\u51E0\u4E2A\u6708\u524D\uFF0C\u6211\u5728\u5BFB\u627E\u4E00\u6B3E\u5408\u9002\u7684\u5DE5\u5177\u65F6\uFF0C\u5076\u7136\u53D1\u73B0\u4E86 {product}\u3002

{tagline}

## \u521D\u4F53\u9A8C

\u7B2C\u4E00\u6B21\u4F7F\u7528 {product}\uFF0C\u6211\u5C31\u88AB\u5B83\u7684\u8BBE\u8BA1\u6240\u5438\u5F15\u3002{description}

## \u65E5\u5E38\u4F7F\u7528

\u73B0\u5728\uFF0C{product} \u5DF2\u7ECF\u6210\u4E3A\u6211\u5DE5\u4F5C\u4E2D\u4E0D\u53EF\u6216\u7F3A\u7684\u4E00\u90E8\u5206\u3002

### \u65E9\u4E0A
\u6253\u5F00 {product}\uFF0C\u5F00\u59CB\u4E00\u5929\u7684\u5DE5\u4F5C\u3002

### \u5DE5\u4F5C\u4E2D
\u5229\u7528\u5404\u79CD\u529F\u80FD\u63D0\u5347\u6548\u7387\u3002

### \u603B\u7ED3
\u5B8C\u6210\u5F53\u5929\u4EFB\u52A1\uFF0C\u505A\u597D\u89C4\u5212\u3002

## \u63A8\u8350\u7406\u7531

\u5982\u679C\u4F60\u4E5F\u5728\u5BFB\u627E\u4E00\u6B3E\u597D\u7528\u7684\u5DE5\u5177\uFF0C\u6211\u771F\u5FC3\u63A8\u8350 {product}\u3002

\u4F53\u9A8C\u5730\u5740\uFF1A{url}`
        },
        listicle: {
          title: "2024 \u5E74\u6700\u503C\u5F97\u5C1D\u8BD5\u7684 {keywords} \u5DE5\u5177\u63A8\u8350\uFF08\u542B {product}\uFF09",
          body: `# 2024 \u5E74\u6700\u503C\u5F97\u5C1D\u8BD5\u7684\u5DE5\u5177\u63A8\u8350

## \u524D\u8A00

\u4ECA\u5929\u7ED9\u5927\u5BB6\u63A8\u8350\u51E0\u6B3E\u4F18\u79C0\u7684\u5DE5\u5177\uFF0C\u5E2E\u52A9\u63D0\u5347\u4F60\u7684\u5DE5\u4F5C\u6548\u7387\u3002

## \u63A8\u8350\u5217\u8868

### 1. {product} \u2B50 \u5F3A\u70C8\u63A8\u8350

{tagline}

{description}

**\u4F18\u70B9\uFF1A**
- \u529F\u80FD\u5F3A\u5927
- \u754C\u9762\u7B80\u6D01
- \u6301\u7EED\u66F4\u65B0

**\u5B98\u7F51\uFF1A** {url}

### 2. \u5176\u4ED6\u5907\u9009\u5DE5\u5177

\u5E02\u9762\u4E0A\u8FD8\u6709\u4E00\u4E9B\u5176\u4ED6\u9009\u62E9\uFF0C\u4F46\u7EFC\u5408\u8003\u8651\u529F\u80FD\u3001\u4EF7\u683C\u548C\u4F53\u9A8C\uFF0C{product} \u662F\u6211\u7684\u9996\u9009\u3002

## \u5982\u4F55\u9009\u62E9

\u9009\u62E9\u5DE5\u5177\u65F6\uFF0C\u5EFA\u8BAE\u8003\u8651\uFF1A
1. \u662F\u5426\u6EE1\u8DB3\u6838\u5FC3\u9700\u6C42
2. \u5B66\u4E60\u6210\u672C
3. \u4EF7\u683C\u548C\u6027\u4EF7\u6BD4

## \u603B\u7ED3

\u5982\u679C\u4F60\u6B63\u5728\u5BFB\u627E\u4E00\u6B3E\u53EF\u9760\u7684\u5DE5\u5177\uFF0C\u4E0D\u59A8\u8BD5\u8BD5 {product}\u3002

\u7ACB\u5373\u4F53\u9A8C\uFF1A{url}`
        }
      };
    } else {
      return {
        tutorial: {
          title: "Complete Guide to {product}: From Beginner to Pro",
          body: `# Complete Guide to {product}

## Introduction

{tagline}

In this comprehensive guide, I'll walk you through everything you need to know about {product}.

## What is {product}?

{description}

## Key Features

### 1. Core Capabilities

{product} offers powerful features:

- **User-friendly**: Intuitive interface
- **Powerful**: Professional-grade capabilities
- **Active development**: Regular updates

### 2. Use Cases

Whether you're an individual or team, {product} fits your needs:

- Daily workflows
- Project management
- Team collaboration

## Getting Started

1. Visit {url}
2. Create an account
3. Start using

## Pro Tips

Master these techniques to boost your productivity:

1. Learn keyboard shortcuts
2. Customize settings
3. Explore advanced features

## Conclusion

{product} is a tool worth trying. {tagline}

Get started: {url}`
        },
        comparison: {
          title: "{product} vs Alternatives: Why I Made the Switch",
          body: `# {product} vs Alternatives

## The Challenge

Choosing the right tool isn't easy. Here's why I ultimately chose {product}.

## About {product}

{description}

## Comparison Points

### Usability
{product} features a clean, intuitive interface.

### Features
Comprehensive functionality for daily needs.

### Value
Better value compared to alternatives.

## Why {product}?

1. **{tagline}**
2. Continuous improvements
3. Active community

## Conclusion

If you're looking for a reliable tool, give {product} a try.

Learn more: {url}`
        },
        problem: {
          title: "How I Solved {keywords} Problems with {product}",
          body: `# Solving {keywords} Problems

## The Struggle

Many of us face these challenges: low efficiency, clunky tools, tedious workflows.

I was there too, until I discovered {product}.

## The Solution

{product} - {tagline}

{description}

## How It Helped

### Problem 1: Efficiency
{product}'s smart features dramatically improved my workflow.

### Problem 2: Usability
Clean design means minimal learning curve.

### Problem 3: Collaboration
Robust collaboration features for teams.

## Results

Since using {product}, my productivity increased by at least 30%.

## Get Started

Visit {url} to try it yourself.

Have similar challenges? Give {product} a try!`
        },
        story: {
          title: "My Journey with {product}",
          body: `# My Journey with {product}

## Discovery

A few months ago, I stumbled upon {product} while searching for the right tool.

{tagline}

## First Impressions

I was immediately impressed by the design. {description}

## Daily Use

Now, {product} is an essential part of my workflow.

### Morning
Open {product} to start the day.

### During Work
Leverage features to stay productive.

### End of Day
Wrap up tasks and plan ahead.

## My Recommendation

If you're looking for a great tool, I genuinely recommend {product}.

Try it: {url}`
        },
        listicle: {
          title: "Best {keywords} Tools in 2024 (Including {product})",
          body: `# Best Tools in 2024

## Introduction

Here are my top tool recommendations to boost your productivity.

## The List

### 1. {product} \u2B50 Highly Recommended

{tagline}

{description}

**Pros:**
- Powerful features
- Clean interface
- Regular updates

**Website:** {url}

### 2. Other Options

There are alternatives, but considering features, price, and experience, {product} is my top pick.

## How to Choose

When selecting tools, consider:
1. Core functionality
2. Learning curve
3. Pricing and value

## Conclusion

Looking for a reliable tool? Give {product} a try.

Get started: {url}`
        }
      };
    }
  }
  function renderArticleVersions() {
    const container = document.getElementById("articleVersions");
    if (!container || generatedArticles.length === 0) return;
    container.innerHTML = generatedArticles.map((article, i) => `
    <button class="article-version-tab ${i === currentArticleIndex ? "active" : ""}" data-index="${i}">
      <span class="platform-icon">${PLATFORM_ICONS[article.platform] || "\u{1F4C4}"}</span>
      <span>${article.platform}</span>
      <span class="word-count">${article.language.toUpperCase()}</span>
    </button>
  `).join("");
    container.querySelectorAll(".article-version-tab").forEach((tab) => {
      tab.addEventListener("click", () => {
        currentArticleIndex = parseInt(tab.dataset.index || "0");
        container.querySelectorAll(".article-version-tab").forEach((t2) => t2.classList.remove("active"));
        tab.classList.add("active");
        renderCurrentArticle();
      });
    });
  }
  function renderCurrentArticle() {
    const container = document.getElementById("articleContent");
    if (!container || !generatedArticles[currentArticleIndex]) return;
    const article = generatedArticles[currentArticleIndex];
    container.innerHTML = `
    <h1 class="article-title">${escapeHtml(article.title)}</h1>
    <div class="article-meta">
      <span class="article-meta-item">\u{1F4DD} ${article.type}</span>
      <span class="article-meta-item">\u{1F310} ${article.platform}</span>
      <span class="article-meta-item">\u{1F4AC} ${article.language.toUpperCase()}</span>
      <span class="article-meta-item">\u{1F4CA} ${article.wordCount} chars</span>
    </div>
    <div class="article-body">${formatArticleBody(article.body)}</div>
    ${article.keywords.length > 0 ? `
      <div class="article-tags">
        ${article.keywords.map((k) => `<span class="article-tag">#${escapeHtml(k)}</span>`).join("")}
      </div>
    ` : ""}
  `;
  }
  function formatArticleBody(body) {
    return escapeHtml(body).replace(/^# (.+)$/gm, "<h1>$1</h1>").replace(/^## (.+)$/gm, "<h2>$1</h2>").replace(/^### (.+)$/gm, "<h3>$1</h3>").replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/\*(.+?)\*/g, "<em>$1</em>").replace(/^- (.+)$/gm, "<li>$1</li>").replace(/(<li>.*<\/li>\n?)+/g, "<ul>$&</ul>").replace(/^(\d+)\. (.+)$/gm, "<li>$2</li>").replace(/\n\n/g, "</p><p>").replace(/^(?!<[hulo])(.+)$/gm, "<p>$1</p>").replace(/<p><\/p>/g, "").replace(/---/g, "<hr>");
  }
  function copyCurrentArticle() {
    if (!generatedArticles[currentArticleIndex]) return;
    const article = generatedArticles[currentArticleIndex];
    const text = `${article.title}

${article.body}`;
    navigator.clipboard.writeText(text).then(() => {
      showToast(t("msg.articleCopied"), "success");
    }).catch(() => {
      showToast("Failed to copy", "error");
    });
  }
  function queueArticleTask() {
    if (generatedArticles.length === 0) {
      showToast("No articles to publish", "error");
      return;
    }
    const platforms = [...new Set(generatedArticles.map((a) => a.platform))];
    const title = `Publish ${generatedArticles.length} articles to ${platforms.join(", ")}`;
    createTask("publish", title, { contents: generatedArticles.map((a) => ({
      platform: a.platform,
      language: a.language,
      product_name: a.productName,
      body: `${a.title}

${a.body}`,
      hashtags: a.keywords
    })) });
    savedArticles.push(...generatedArticles);
    renderSavedArticles();
    showToast("Articles added to task queue", "success");
    navigateTo("tasks");
  }
  function renderSavedArticles() {
    const container = document.getElementById("savedArticlesList");
    if (!container) return;
    if (savedArticles.length === 0) {
      container.innerHTML = '<p class="text-muted">No saved articles yet</p>';
      return;
    }
    container.innerHTML = savedArticles.slice(0, 10).map((article) => `
    <div class="saved-article-item">
      <div class="saved-article-info">
        <div class="saved-article-title">${escapeHtml(article.title)}</div>
        <div class="saved-article-meta">
          <span>${article.platform}</span>
          <span>${article.language.toUpperCase()}</span>
          <span>${formatTimeAgo(article.createdAt)}</span>
        </div>
      </div>
      <div class="saved-article-actions">
        <button class="btn btn-small btn-secondary" onclick="viewSavedArticle('${article.id}')">View</button>
      </div>
    </div>
  `).join("");
  }
  window.viewSavedArticle = function(id) {
    const article = savedArticles.find((a) => a.id === id);
    if (article) {
      generatedArticles = [article];
      currentArticleIndex = 0;
      renderArticleVersions();
      renderCurrentArticle();
    }
  };
  function generateTaskId() {
    return "task_" + Date.now() + "_" + Math.random().toString(36).substr(2, 9);
  }
  function createTask(type, title, data) {
    const task = {
      id: generateTaskId(),
      type,
      title,
      status: "pending",
      progress: 0,
      total: 0,
      createdAt: /* @__PURE__ */ new Date(),
      data
    };
    tasks.unshift(task);
    renderTasks();
    processTaskQueue();
    return task;
  }
  function updateTask(taskId, updates) {
    const task = tasks.find((t2) => t2.id === taskId);
    if (task) {
      Object.assign(task, updates);
      renderTasks();
    }
  }
  async function processTaskQueue() {
    if (taskRunning) return;
    const pendingTask = tasks.find((t2) => t2.status === "pending");
    if (!pendingTask) return;
    taskRunning = true;
    updateTask(pendingTask.id, { status: "running" });
    try {
      switch (pendingTask.type) {
        case "publish":
          await executePublishTask(pendingTask);
          break;
        case "reply":
          await executeReplyTask(pendingTask);
          break;
        case "discover":
          await executeDiscoverTask(pendingTask);
          break;
        case "nurture":
          await executeNurtureTask(pendingTask);
          break;
      }
      updateTask(pendingTask.id, { status: "completed", completedAt: /* @__PURE__ */ new Date() });
    } catch (error) {
      updateTask(pendingTask.id, { status: "failed", error: error?.toString() });
    }
    taskRunning = false;
    setTimeout(processTaskQueue, 500);
  }
  async function executePublishTask(task) {
    const contents = task.data.contents;
    task.total = contents.length;
    updateTask(task.id, { total: contents.length });
    let successCount = 0;
    let failCount = 0;
    const results = [];
    const platformContents = {};
    contents.forEach((c) => {
      if (!platformContents[c.platform]) {
        platformContents[c.platform] = [];
      }
      platformContents[c.platform].push(c);
    });
    const platformAccounts = {};
    try {
      const accountsData = await invoke2("list_accounts");
      (accountsData || []).forEach((acc) => {
        if (acc.status === "active") {
          if (!platformAccounts[acc.platform]) {
            platformAccounts[acc.platform] = [];
          }
          platformAccounts[acc.platform].push(acc);
        }
      });
    } catch (e) {
      console.warn("Could not load accounts for rotation:", e);
    }
    let currentIndex = 0;
    for (const content of contents) {
      currentIndex++;
      updateTask(task.id, { progress: currentIndex });
      try {
        const platformAccs = platformAccounts[content.platform] || [];
        let selectedAccount = null;
        if (platformAccs.length > 0) {
          const accIndex = platformContents[content.platform].indexOf(content) % platformAccs.length;
          selectedAccount = platformAccs[accIndex];
        }
        const publishContent = {
          ...content,
          account_id: selectedAccount?.id || null
        };
        const result = await invoke2("publish_content", { content: publishContent });
        if (result.success) {
          successCount++;
          results.push({ platform: content.platform, success: true, url: result.post_url });
        } else {
          failCount++;
          results.push({ platform: content.platform, success: false, error: result.error });
          if (platformAccs.length > 1) {
            await new Promise((resolve) => setTimeout(resolve, 5e3));
            const retryAccount = platformAccs[(platformContents[content.platform].indexOf(content) + 1) % platformAccs.length];
            const retryContent = { ...content, account_id: retryAccount?.id };
            try {
              const retryResult = await invoke2("publish_content", { content: retryContent });
              if (retryResult.success) {
                successCount++;
                failCount--;
                results[results.length - 1] = { platform: content.platform, success: true, url: retryResult.post_url, retried: true };
              }
            } catch (retryError) {
              console.error(`Retry also failed for ${content.platform}:`, retryError);
            }
          }
        }
      } catch (error) {
        failCount++;
        results.push({ platform: content.platform, success: false, error: String(error) });
        console.error(`Failed to publish to ${content.platform}:`, error);
      }
      if (currentIndex < contents.length) {
        const delay = getPublishDelay(content.platform);
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }
    task.data.results = results;
    task.data.successCount = successCount;
    task.data.failCount = failCount;
    if (failCount > 0 && successCount === 0) {
      showToast(`Publishing failed for all ${failCount} posts`, "error");
    } else if (failCount > 0) {
      showToast(`Published ${successCount}/${contents.length} posts (${failCount} failed)`, "warning");
    } else {
      showToast(`Successfully published ${successCount} posts!`, "success");
    }
  }
  function getPublishDelay(platform) {
    const delays = {
      twitter: 3e3,
      // Twitter is more lenient
      reddit: 12e4,
      // Reddit needs longer waits (2 min)
      linkedin: 3e4,
      // LinkedIn 30 sec
      zhihu: 6e4,
      // 知乎 1 min
      weibo: 3e4,
      // 微博 30 sec
      hackernews: 12e4,
      // HN is strict
      producthunt: 6e4,
      medium: 3e4,
      devto: 3e4
    };
    return delays[platform.toLowerCase()] || 5e3;
  }
  async function executeReplyTask(task) {
    const { postId, productId, replyContent } = task.data;
    task.total = 1;
    updateTask(task.id, { total: 1 });
    await invoke2("reply_to_post", { postId, productId, customReply: replyContent });
    updateTask(task.id, { progress: 1 });
  }
  async function executeDiscoverTask(task) {
    const { keywords: keywords2 } = task.data;
    task.total = keywords2.length;
    updateTask(task.id, { total: keywords2.length });
    for (let i = 0; i < keywords2.length; i++) {
      const keyword = keywords2[i];
      updateTask(task.id, { progress: i + 1 });
      for (const platform of keyword.platforms) {
        try {
          await invoke2("discover_posts", { platform, keyword: keyword.keyword });
        } catch (error) {
          console.error(`Failed to discover on ${platform}:`, error);
        }
        await new Promise((resolve) => setTimeout(resolve, 2e3));
      }
    }
  }
  async function executeNurtureTask(task) {
    const { accountIds, seconds, continuous } = task.data;
    task.total = accountIds.length;
    updateTask(task.id, { total: accountIds.length });
    let successCount = 0;
    let failCount = 0;
    const results = [];
    for (let i = 0; i < accountIds.length; i++) {
      const accountId = accountIds[i];
      updateTask(task.id, { progress: i + 1 });
      try {
        const result = await invoke2("quick_nurture", {
          accountId,
          seconds: seconds || 60
        });
        successCount++;
        results.push({ accountId, success: true, duration: seconds });
        if (i < accountIds.length - 1) {
          await new Promise((resolve) => setTimeout(resolve, 3e3));
        }
      } catch (error) {
        failCount++;
        results.push({ accountId, success: false, error: String(error) });
        console.error(`Nurture failed for account ${accountId}:`, error);
      }
    }
    task.data.results = results;
    task.data.successCount = successCount;
    task.data.failCount = failCount;
    if (continuous && successCount > 0) {
      setTimeout(() => {
        createTask("nurture", `\u{1F331} ${t("nurture.title")} (Loop)`, {
          accountIds,
          seconds,
          continuous: true
        });
      }, 5 * 60 * 1e3);
    }
    await loadAccounts();
  }
  window.createNurtureTask = function(accountIds, seconds, continuous = false) {
    if (!accountIds || accountIds.length === 0) {
      showToast(t("nurture.noAccounts"), "error");
      return;
    }
    const task = createTask("nurture", `\u{1F331} ${t("nurture.title")} (${accountIds.length} ${t("msg.account")})`, {
      accountIds,
      seconds,
      continuous
    });
    showToast(`${t("msg.taskAdded")}: ${accountIds.length} ${t("msg.account")}`, "success");
    navigateTo("tasks");
    return task;
  };
  var engineControlsWired = false;
  var tasksPollTimer;
  function loadTasksPage() {
    wireEngineControls();
    refreshTasksPage();
    if (tasksPollTimer) clearInterval(tasksPollTimer);
    tasksPollTimer = window.setInterval(() => {
      if (currentPage === "tasks") refreshTasksPage();
      else if (tasksPollTimer) {
        clearInterval(tasksPollTimer);
        tasksPollTimer = void 0;
      }
    }, 3e3);
  }
  function wireEngineControls() {
    if (engineControlsWired) return;
    engineControlsWired = true;
    document.getElementById("btnStartEngine")?.addEventListener("click", async () => {
      try {
        await invoke2("start_engine");
        showToast("\u5F15\u64CE\u5DF2\u542F\u52A8", "success");
      } catch (e) {
        showToast("\u542F\u52A8\u5931\u8D25: " + e, "error");
      }
      setTimeout(refreshTasksPage, 400);
    });
    document.getElementById("btnStopEngine")?.addEventListener("click", async () => {
      try {
        await invoke2("stop_engine");
        showToast("\u5F15\u64CE\u505C\u6B62\u4E2D\u2026", "info");
      } catch (e) {
        showToast("\u505C\u6B62\u5931\u8D25: " + e, "error");
      }
      setTimeout(refreshTasksPage, 400);
    });
    document.getElementById("btnSelCheck")?.addEventListener("click", async () => {
      const platform = document.getElementById("selCheckPlatform")?.value?.trim();
      const keyword = document.getElementById("selCheckKeyword")?.value?.trim();
      const out = document.getElementById("selCheckResult");
      if (!platform) {
        showToast("\u8BF7\u586B\u5199\u5E73\u53F0", "error");
        return;
      }
      if (out) out.textContent = `\u68C0\u6D4B\u4E2D\uFF08\u542F\u52A8 profile + \u5BFC\u822A ${platform} \u641C\u7D22\u9875\uFF0C\u7EA6 6 \u79D2\uFF09\u2026`;
      try {
        const r = await invoke2("check_selector", { platform, keyword: keyword || void 0 });
        if (out) out.textContent = `\u5E73\u53F0: ${r.platform}
\u641C\u7D22\u9875: ${r.search_url}
\u9875\u9762\u6807\u9898: ${r.page_title}
\u4F7F\u7528 profile: ${r.profile_used}
\u9009\u62E9\u5668: ${r.selector}
\u9875\u9762\u603B\u94FE\u63A5: ${r.total_links}    \u547D\u4E2D: ${r.matched}
\u7ED3\u8BBA: ${r.note}
` + (r.samples.length ? `\u6837\u672C:
  ${r.samples.join("\n  ")}` : "\u6837\u672C: (\u65E0)");
      } catch (e) {
        if (out) out.textContent = "\u68C0\u6D4B\u5931\u8D25: " + e;
      }
    });
    document.getElementById("replyModeSelect")?.addEventListener("change", async (ev) => {
      const mode = ev.target.value;
      try {
        await invoke2("set_engine_reply_mode", { mode });
        showToast(mode === "auto" ? "\u5DF2\u5207\u6362\u4E3A\u5168\u81EA\u52A8\uFF1A\u5F15\u64CE\u5C06\u76F4\u63A5\u53D1\u5E03\u56DE\u590D" : "\u5DF2\u5207\u6362\u4E3A\u534A\u81EA\u52A8\uFF1A\u56DE\u590D\u8FDB\u5165\u5BA1\u6838\u961F\u5217\u7B49\u4F60\u6279\u51C6", "success");
      } catch (e) {
        showToast("\u5207\u6362\u5931\u8D25: " + e, "error");
      }
    });
  }
  async function refreshTasksPage() {
    try {
      const [status, dbTasks, replyMode, pending, nurture, leads, mstats] = await Promise.all([
        invoke2("get_engine_status"),
        invoke2("list_tasks", {}),
        invoke2("get_engine_reply_mode").catch(() => "review"),
        invoke2("list_pending_replies").catch(() => []),
        invoke2("get_nurture_overview").catch(() => []),
        invoke2("list_leads", {}).catch(() => []),
        invoke2("get_marketing_stats").catch(() => null)
      ]);
      const sel = document.getElementById("replyModeSelect");
      if (sel && document.activeElement !== sel) sel.value = replyMode;
      renderPendingReplies(pending);
      renderNurtureOverview(nurture);
      renderLeads(leads);
      if (mstats) renderMarketingStats(mstats);
      const sp = document.getElementById("taskStatPending");
      const sr = document.getElementById("taskStatRunning");
      const sc = document.getElementById("taskStatCompleted");
      if (sp) sp.textContent = `${status.pending} pending`;
      if (sr) sr.textContent = `${status.running_count} running`;
      if (sc) sc.textContent = `${status.completed + status.failed + status.blocked} done`;
      const es = document.getElementById("engineState");
      if (es) es.textContent = status.running ? `\u8FD0\u884C\u4E2D (\u5DF2\u5904\u7406 ${status.processed})` : "\u5DF2\u505C\u6B62";
      renderDbTasks(dbTasks);
    } catch (_e) {
      const es = document.getElementById("engineState");
      if (es) es.textContent = "\u4E0D\u53EF\u7528 (\u9700\u684C\u9762\u5E94\u7528)";
    }
  }
  function renderDbTasks(list) {
    const container = document.getElementById("tasksList");
    if (!container) return;
    if (!list.length) {
      container.innerHTML = `<div class="tasks-empty"><div class="tasks-empty-icon">\u{1F4CB}</div><p>\u6682\u65E0\u4EFB\u52A1</p>
      <p class="text-muted">\u5728\u300C\u63A8\u5E7F\u6D3B\u52A8\u300D\u521B\u5EFA\u5E76\u542F\u52A8 Campaign \u4F1A\u751F\u6210\u4EFB\u52A1\uFF0C\u542F\u52A8\u5F15\u64CE\u540E\u81EA\u52A8\u6267\u884C</p></div>`;
      return;
    }
    const icon = {
      pending: "\u23F3",
      running: "\u26A1",
      completed: "\u2705",
      failed: "\u274C",
      blocked: "\u26A0\uFE0F",
      cancelled: "\u{1F6AB}"
    };
    const typeLabel = {
      article: "\u{1F4DD} \u53D1\u5E03",
      post: "\u{1F4DD} \u53D1\u5E03",
      publish: "\u{1F4DD} \u53D1\u5E03",
      tweet: "\u{1F4DD} \u53D1\u5E03",
      reply: "\u{1F50D} \u5173\u952E\u8BCD\u56DE\u590D",
      reply_keyword: "\u{1F50D} \u5173\u952E\u8BCD\u56DE\u590D",
      engage: "\u{1F50D} \u5173\u952E\u8BCD\u56DE\u590D",
      reply_mention: "\u{1F4AC} \u8BC4\u8BBA\u56DE\u590D"
    };
    container.innerHTML = list.map((t2) => {
      const ttypeText = typeLabel[t2.task_type] || t2.task_type;
      const actions = [];
      if (t2.status === "blocked") actions.push(`<button class="btn btn-small btn-primary" onclick="unblockTask('${t2.id}')">\u89E3\u9664\u963B\u585E</button>`);
      if (t2.status === "failed" || t2.status === "cancelled" || t2.status === "blocked") actions.push(`<button class="btn btn-small btn-secondary" onclick="retryTask('${t2.id}')">\u91CD\u8BD5</button>`);
      if (t2.status === "pending" || t2.status === "blocked") actions.push(`<button class="btn btn-small btn-secondary" onclick="cancelTask('${t2.id}')">\u53D6\u6D88</button>`);
      const reason = t2.error_message ? `<div class="text-muted" style="font-size:12px;color:var(--warning,#e0a800);margin-top:4px;">${escapeHtml(t2.error_message)}</div>` : "";
      return `<div class="task-item" style="padding:12px;border-bottom:1px solid var(--border,#eee);">
      <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px;">
        <div>
          <span style="font-size:16px;">${icon[t2.status] || "\u2022"}</span>
          <strong>${escapeHtml(ttypeText)}</strong>
          <span class="text-muted">${escapeHtml(t2.platform || "\u2014")}</span>
          <span class="task-stat">${escapeHtml(t2.status)}</span>
          ${t2.retry_count ? `<span class="text-muted">retry ${t2.retry_count}</span>` : ""}
          ${reason}
        </div>
        <div class="btn-group">${actions.join("")}</div>
      </div>
    </div>`;
    }).join("");
  }
  function intentBadge(score) {
    const s = score ?? 0;
    const color = s >= 70 ? "#16a34a" : s >= 40 ? "#d97706" : "#6b7280";
    return `<span class="task-stat" style="background:${color};color:#fff;" title="\u4E70\u5BB6\u610F\u5411\u5206">\u610F\u5411 ${s}</span>`;
  }
  function renderPendingReplies(list) {
    const badge = document.getElementById("reviewCount");
    if (badge) badge.textContent = String(list.length);
    const c = document.getElementById("reviewList");
    if (!c) return;
    if (!list.length) {
      c.innerHTML = `<div class="text-muted" style="padding:12px;font-size:13px;">\u6682\u65E0\u5F85\u5BA1\u56DE\u590D\u3002\u534A\u81EA\u52A8\u6A21\u5F0F\u4E0B\uFF0C\u5F15\u64CE\u8BFB\u53D6\u771F\u5B9E\u5E16\u5B50\u3001AI \u751F\u6210\u56DE\u590D\u540E\u4F1A\u5728\u8FD9\u91CC\u7B49\u4F60\u6279\u51C6\uFF1B\u5168\u81EA\u52A8\u6A21\u5F0F\u5219\u76F4\u63A5\u53D1\u5E03\u3002</div>`;
      return;
    }
    c.innerHTML = list.map((r) => `
    <div class="task-item" style="padding:12px;border-bottom:1px solid var(--border,#eee);">
      <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:8px;">
        <div>
          <span class="task-stat">${escapeHtml(r.platform)}</span>
          <span class="text-muted" style="font-size:12px;">${escapeHtml(r.reply_type === "mention" ? "\u8BC4\u8BBA\u56DE\u590D" : "\u5173\u952E\u8BCD\u56DE\u590D")}</span>
          ${intentBadge(r.intent_score)}
          ${r.product_mentioned ? '<span class="task-stat" style="background:#2563eb;color:#fff;">\u8F6F\u63D0\u4EA7\u54C1</span>' : ""}
          <a href="${escapeHtml(r.post_url)}" target="_blank" class="text-muted" style="font-size:12px;">\u539F\u5E16 \u2197</a>
        </div>
        <div class="btn-group">
          <button class="btn btn-small btn-primary" onclick="approveReply('${r.id}')">\u6279\u51C6\u53D1\u5E03</button>
          <button class="btn btn-small btn-secondary" onclick="rejectReply('${r.id}')">\u9A73\u56DE</button>
        </div>
      </div>
      ${r.post_title || r.post_content ? `<div class="text-muted" style="font-size:12px;margin-top:4px;">\u5E16\u5B50\uFF1A${escapeHtml(r.post_title && r.post_title.trim() ? r.post_title : (r.post_content || "").slice(0, 160))}</div>` : ""}
      <textarea id="rv_${r.id}" style="width:100%;box-sizing:border-box;margin-top:6px;min-height:62px;font-size:13px;padding:6px;border:1px solid var(--border,#ddd);border-radius:6px;">${escapeHtml(r.reply_content)}</textarea>
      ${r.reason ? `<div class="text-muted" style="font-size:11px;margin-top:2px;">AI \u5224\u5B9A\uFF1A${escapeHtml(r.reason)}</div>` : ""}
    </div>`).join("");
  }
  window.approveReply = async (id) => {
    const ta = document.getElementById("rv_" + id);
    const editedContent = ta?.value;
    try {
      const r = await invoke2("approve_reply", { id, editedContent });
      if (r.success) showToast("\u5DF2\u53D1\u5E03\u56DE\u590D", "success");
      else showToast("\u53D1\u5E03\u5931\u8D25: " + (r.error || "\u672A\u77E5\u9519\u8BEF"), "error");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.rejectReply = async (id) => {
    try {
      await invoke2("reject_reply", { id });
      showToast("\u5DF2\u9A73\u56DE\uFF0C\u8BE5\u5E16\u8DF3\u8FC7", "info");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  function renderNurtureOverview(list) {
    const c = document.getElementById("nurtureOverview");
    if (!c) return;
    if (!list.length) {
      c.innerHTML = `<div class="text-muted" style="padding:12px;font-size:13px;">\u6682\u65E0\u8D26\u53F7\u3002\u517B\u53F7\u8C03\u5EA6\u5F15\u64CE\u4F1A\u6309\u5E73\u53F0\u7B56\u7565 + \u53F7\u9F84\u5206\u671F\uFF0C\u5728\u6D3B\u8DC3\u65F6\u6BB5\u81EA\u52A8\u7ED9\u5DF2\u7ED1\u5B9A profile \u7684\u8D26\u53F7\u5B89\u6392\u6D4F\u89C8\uFF0C\u63D0\u5347\u6743\u91CD\u3001\u964D\u4F4E\u5C01\u53F7\u98CE\u9669\u3002</div>`;
      return;
    }
    const healthBadge = {
      healthy: '<span class="task-stat" style="background:#16a34a;color:#fff;">\u5065\u5EB7</span>',
      logged_out: '<span class="task-stat" style="background:#dc2626;color:#fff;">\u6389\u767B\u5F55</span>',
      shadowbanned: '<span class="task-stat" style="background:#dc2626;color:#fff;">\u7591\u4F3C\u5C01\u7981</span>',
      banned: '<span class="task-stat" style="background:#dc2626;color:#fff;">\u5DF2\u5C01</span>',
      unknown: '<span class="task-stat" style="background:#6b7280;color:#fff;">\u5F85\u4F53\u68C0</span>'
    };
    const phaseLabel = { warmup: "\u{1F423} \u65B0\u53F7\u671F", growth: "\u{1F4C8} \u6210\u957F\u671F", mature: "\u{1F333} \u6210\u719F\u671F", "\u2014": "\u2014 \u65E0\u7B56\u7565" };
    const fmt = (s) => s >= 3600 ? `${(s / 3600).toFixed(1)}h` : `${Math.round(s / 60)}m`;
    const rows = list.map((a) => {
      const pct = a.today_target > 0 ? Math.min(100, Math.round(a.today_done / a.today_target * 100)) : 0;
      const prog = a.today_target > 0 ? `<div style="display:inline-block;width:90px;height:8px;background:#e5e7eb;border-radius:4px;vertical-align:middle;overflow:hidden;"><div style="width:${pct}%;height:100%;background:#16a34a;"></div></div> ${a.today_done}/${a.today_target}` : '<span class="text-muted">\u2014</span>';
      return `<tr style="border-bottom:1px solid var(--border,#eee);">
      <td style="padding:6px 8px;">${escapeHtml(a.platform)} ${a.bound ? "" : '<span class="text-muted" style="font-size:11px;">(\u672A\u7ED1\u5B9A)</span>'}</td>
      <td style="padding:6px 8px;">${phaseLabel[a.phase] || a.phase} <span class="text-muted" style="font-size:11px;">${a.age_days}\u5929</span></td>
      <td style="padding:6px 8px;">${prog}</td>
      <td style="padding:6px 8px;">${fmt(a.total_seconds)}</td>
      <td style="padding:6px 8px;">${healthBadge[a.health_status] || healthBadge.unknown}</td>
    </tr>`;
    }).join("");
    c.innerHTML = `<table style="width:100%;border-collapse:collapse;font-size:13px;">
    <thead><tr style="text-align:left;color:var(--text-muted,#888);">
      <th style="padding:4px 8px;">\u5E73\u53F0</th><th style="padding:4px 8px;">\u5206\u671F/\u53F7\u9F84</th>
      <th style="padding:4px 8px;">\u4ECA\u65E5\u8FDB\u5EA6</th><th style="padding:4px 8px;">\u7D2F\u8BA1</th><th style="padding:4px 8px;">\u5065\u5EB7</th>
    </tr></thead><tbody>${rows}</tbody></table>`;
  }
  function renderLeads(list) {
    const badge = document.getElementById("leadsCount");
    if (badge) badge.textContent = String(list.length);
    const c = document.getElementById("leadsList");
    if (!c) return;
    if (!list.length) {
      c.innerHTML = `<div class="text-muted" style="padding:12px;font-size:13px;">\u6682\u65E0\u7EBF\u7D22\u3002\u6BCF\u6761\u771F\u5B9E\u53D1\u51FA\u7684\u56DE\u590D\u90FD\u4F1A\u751F\u6210\u4E00\u6761\u7EBF\u7D22\uFF0C\u5728\u8FD9\u91CC\u8DDF\u8E2A\u5BF9\u65B9\u662F\u5426\u56DE\u5E94\u3001\u662F\u5426\u8F6C\u5316\u3002</div>`;
      return;
    }
    const statusLabel = {
      engaged: "\u5DF2\u89E6\u8FBE",
      replied_back: "\u5DF2\u56DE\u5E94",
      converted: "\u2705 \u5DF2\u8F6C\u5316",
      dismissed: "\u5DF2\u5FFD\u7565"
    };
    c.innerHTML = list.map((l) => `
    <div class="task-item" style="padding:10px 12px;border-bottom:1px solid var(--border,#eee);">
      <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:8px;">
        <div style="min-width:0;">
          <span class="task-stat">${escapeHtml(l.platform)}</span>
          ${intentBadge(l.intent_score)}
          <span class="task-stat" style="background:#334155;color:#fff;">${statusLabel[l.status] || l.status}</span>
          ${l.author ? `<span class="text-muted" style="font-size:12px;">${escapeHtml(l.author.slice(0, 40))}</span>` : ""}
          ${l.post_url ? `<a href="${escapeHtml(l.post_url)}" target="_blank" class="text-muted" style="font-size:12px;">\u539F\u5E16\u2197</a>` : ""}
          ${l.our_reply ? `<div class="text-muted" style="font-size:12px;margin-top:3px;">\u6211\u65B9\uFF1A${escapeHtml(l.our_reply.slice(0, 120))}</div>` : ""}
        </div>
        <div class="btn-group" style="flex-shrink:0;">
          <button class="btn btn-small btn-primary" onclick="markLead('${l.id}','converted')">\u8F6C\u5316</button>
          <button class="btn btn-small btn-secondary" onclick="markLead('${l.id}','replied_back')">\u5DF2\u56DE\u5E94</button>
          <button class="btn btn-small btn-secondary" onclick="markLead('${l.id}','dismissed')">\u5FFD\u7565</button>
        </div>
      </div>
    </div>`).join("");
  }
  window.markLead = async (id, status) => {
    try {
      await invoke2("update_lead_status", { id, status });
      showToast("\u7EBF\u7D22\u5DF2\u66F4\u65B0", "success");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  function renderMarketingStats(s) {
    const c = document.getElementById("marketingStats");
    if (!c) return;
    const t2 = s.totals;
    const card = (label, val, hl) => `<div style="flex:1;min-width:80px;text-align:center;padding:6px;background:var(--bg-subtle,#f8f8f8);border-radius:8px;">
       <div style="font-size:20px;font-weight:700;${hl ? "color:#16a34a;" : ""}">${val}</div>
       <div class="text-muted" style="font-size:11px;">${label}</div></div>`;
    const platRows = s.by_platform.filter((p) => p.discovered > 0 || p.leads > 0).map((p) => `<tr style="border-bottom:1px solid var(--border,#eee);">
      <td style="padding:4px 8px;">${escapeHtml(p.platform)}</td>
      <td style="padding:4px 8px;">${p.discovered}</td>
      <td style="padding:4px 8px;">${p.skipped}</td>
      <td style="padding:4px 8px;">${p.replied}</td>
      <td style="padding:4px 8px;">${p.avg_intent}</td>
      <td style="padding:4px 8px;">${p.leads}</td>
      <td style="padding:4px 8px;color:#16a34a;">${p.converted}</td>
    </tr>`).join("");
    const kw = s.top_keywords.length ? s.top_keywords.map(([k, n, ai]) => `<span class="task-stat" title="\u53D1\u73B0 ${n}\uFF0C\u5E73\u5747\u610F\u5411 ${ai}">${escapeHtml(k)} \xB7 ${n}/\u610F\u5411${ai}</span>`).join(" ") : '<span class="text-muted" style="font-size:12px;">\u6682\u65E0</span>';
    c.innerHTML = `
    <div style="display:flex;gap:8px;flex-wrap:wrap;margin-bottom:8px;">
      ${card("\u53D1\u73B0\u5E16\u5B50", t2.discovered)}${card("\u5DF2\u8DF3\u8FC7", t2.skipped)}${card("\u5DF2\u56DE\u590D", t2.replied)}
      ${card("\u5F85\u5BA1", t2.pending_review)}${card("\u7EBF\u7D22", t2.leads)}${card("\u8F6C\u5316", t2.converted, true)}
    </div>
    <table style="width:100%;border-collapse:collapse;font-size:12px;">
      <thead><tr style="text-align:left;color:var(--text-muted,#888);">
        <th style="padding:4px 8px;">\u5E73\u53F0</th><th style="padding:4px 8px;">\u53D1\u73B0</th><th style="padding:4px 8px;">\u8DF3\u8FC7</th>
        <th style="padding:4px 8px;">\u56DE\u590D</th><th style="padding:4px 8px;">\u5747\u610F\u5411</th><th style="padding:4px 8px;">\u7EBF\u7D22</th><th style="padding:4px 8px;">\u8F6C\u5316</th>
      </tr></thead><tbody>${platRows}</tbody></table>
    <div style="margin-top:8px;"><span class="text-muted" style="font-size:12px;">\u70ED\u95E8\u5173\u952E\u8BCD\uFF1A</span> ${kw}</div>`;
  }
  var mpProductId = "";
  var mpSubmissions = [];
  var mpWired = false;
  async function loadMarketplacesPage() {
    if (!mpWired) {
      mpWired = true;
      document.getElementById("btnMpRefresh")?.addEventListener("click", () => refreshMarketplaces());
      document.getElementById("mpProductSelect")?.addEventListener("change", (e) => {
        mpProductId = e.target.value;
        refreshMarketplaces();
      });
      document.getElementById("btnMpSaveRepo")?.addEventListener("click", async () => {
        const repoUrl = document.getElementById("mpRepoUrl")?.value?.trim();
        const installCmd = document.getElementById("mpInstallCmd")?.value?.trim();
        if (!mpProductId || !repoUrl) {
          showToast("\u8BF7\u9009\u62E9\u4EA7\u54C1\u5E76\u586B\u4ED3\u5E93\u5730\u5740", "error");
          return;
        }
        try {
          await invoke2("set_product_repo", { productId: mpProductId, repoUrl, installCmd });
          showToast("\u5DF2\u4FDD\u5B58\u4ED3\u5E93\u4FE1\u606F", "success");
        } catch (e) {
          showToast("" + e, "error");
        }
      });
      document.getElementById("btnMpCopyListing")?.addEventListener("click", () => {
        const body = document.getElementById("mpListingBody")?.textContent || "";
        navigator.clipboard?.writeText(body);
        showToast("\u5DF2\u590D\u5236", "success");
      });
    }
    try {
      const products2 = await invoke2("list_products");
      const sel = document.getElementById("mpProductSelect");
      if (sel) {
        sel.innerHTML = (products2 || []).map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
        if (!mpProductId && products2 && products2.length) mpProductId = products2[0].id;
        if (mpProductId) sel.value = mpProductId;
        const cur = (products2 || []).find((p) => p.id === mpProductId);
        document.getElementById("mpRepoUrl").value = cur?.repo_url || "";
        document.getElementById("mpInstallCmd").value = cur?.install_cmd || "";
      }
    } catch (e) {
      console.error(e);
    }
    refreshMarketplaces();
  }
  async function refreshMarketplaces() {
    if (!mpProductId) {
      const c = document.getElementById("mpList");
      if (c) c.innerHTML = '<p class="text-muted" style="padding:12px;">\u8BF7\u5148\u5728\u300C\u4EA7\u54C1\u300D\u9875\u521B\u5EFA\u4E00\u4E2A\u4EA7\u54C1\u3002</p>';
      return;
    }
    try {
      mpSubmissions = await invoke2("list_marketplace_submissions", { productId: mpProductId });
    } catch (e) {
      mpSubmissions = [];
    }
    renderMpList();
  }
  function renderMpList() {
    const c = document.getElementById("mpList");
    if (!c) return;
    const methodLabel = { form: "\u8868\u5355(\u53EF\u81EA\u52A8\u9884\u586B)", github_pr: "GitHub PR", cli: "CLI/\u6CE8\u518C", auto_index: "GitHub \u81EA\u52A8\u7D22\u5F15" };
    const statusLabel = {
      pending: "\u672A\u5F00\u59CB",
      materials_ready: "\u8D44\u6599\u5DF2\u5C31\u7EEA",
      submitting: "\u63D0\u4EA4\u4E2D",
      prefilled: "\u5DF2\u586B\u597D\u5F85\u4EBA\u5DE5\u6838\u5BF9",
      needs_review: "\u26A0\uFE0F\u9700\u4EBA\u5DE5(\u767B\u5F55/\u5FC5\u586B/\u9A8C\u8BC1\u7801)",
      submitted: "\u2705\u5DF2\u63D0\u4EA4",
      listed: "\u2705\u5DF2\u4E0A\u67B6",
      failed: "\u5931\u8D25",
      skipped: "\u8DF3\u8FC7"
    };
    const groups = { mcp: [], skill: [], both: [] };
    mpSubmissions.forEach((s) => (groups[s.kind] || (groups[s.kind] = [])).push(s));
    const section = (title, list) => !list.length ? "" : `
    <div class="card" style="margin:12px 0;padding:12px;">
      <strong>${title}</strong>
      <div style="margin-top:8px;">${list.map(rowHtml).join("")}</div>
    </div>`;
    c.innerHTML = section("\u{1F50C} MCP \u5E02\u573A", groups.mcp) + section("\u{1F9E9} Skill \u5E02\u573A", groups.skill) + section("\u{1F501} \u901A\u7528", groups.both);
    function rowHtml(s) {
      const hasListing = !!s.listing;
      const actions = [
        `<button class="btn btn-small btn-primary" onclick="mpGenerate('${s.marketplace_id}')">${hasListing ? "\u91CD\u751F\u6210\u8D44\u6599" : "\u751F\u6210\u8D44\u6599"}</button>`
      ];
      if (hasListing) actions.push(`<button class="btn btn-small btn-secondary" onclick="mpView('${s.marketplace_id}')">\u67E5\u770B\u8D44\u6599</button>`);
      if (s.submit_method === "form") actions.push(`<button class="btn btn-small btn-success" onclick="mpSubmit('${s.marketplace_id}')" ${hasListing ? "" : "disabled"}>\u{1F916} \u81EA\u52A8\u63D0\u4EA4</button>`);
      if (s.submit_url) actions.push(`<a class="btn btn-small btn-secondary" href="${escapeHtml(s.submit_url)}" target="_blank">\u6253\u5F00\u63D0\u4EA4\u9875\u2197</a>`);
      actions.push(`<button class="btn btn-small btn-secondary" onclick="mpMark('${s.marketplace_id}','listed')">\u6807\u8BB0\u5DF2\u4E0A\u67B6</button>`);
      return `<div class="task-item" style="padding:8px 10px;border-bottom:1px solid var(--border,#eee);">
      <div style="display:flex;justify-content:space-between;align-items:center;gap:8px;flex-wrap:wrap;">
        <div>
          <strong>${escapeHtml(s.marketplace_name)}</strong>
          <span class="task-stat">${methodLabel[s.submit_method] || s.submit_method}</span>
          <span class="task-stat" style="background:${s.status === "listed" ? "#16a34a" : s.status === "pending" ? "#6b7280" : "#2563eb"};color:#fff;">${statusLabel[s.status] || s.status}</span>
          ${s.notes ? `<span class="text-muted" style="font-size:11px;">${escapeHtml(s.notes)}</span>` : ""}
        </div>
        <div class="btn-group" style="flex-wrap:wrap;">${actions.join("")}</div>
      </div>
    </div>`;
    }
  }
  window.mpGenerate = async (mid) => {
    showToast("AI \u6B63\u5728\u751F\u6210\u4E0A\u67B6\u8D44\u6599\u2026", "info");
    try {
      await invoke2("generate_marketplace_listing", { productId: mpProductId, marketplaceId: mid });
      showToast("\u8D44\u6599\u5DF2\u751F\u6210", "success");
      refreshMarketplaces();
    } catch (e) {
      showToast("\u751F\u6210\u5931\u8D25: " + e, "error");
    }
  };
  window.mpView = (mid) => {
    const s = mpSubmissions.find((x) => x.marketplace_id === mid);
    if (!s) return;
    document.getElementById("mpListingTitle").textContent = `${s.marketplace_name} \u2014 \u4E0A\u67B6\u8D44\u6599`;
    document.getElementById("mpListingBody").textContent = s.listing || "(\u65E0)";
    document.getElementById("mpListingModal").style.display = "flex";
  };
  window.mpSubmit = async (mid) => {
    try {
      const msg = await invoke2("submit_marketplace", { productId: mpProductId, marketplaceId: mid });
      showToast(msg, "success");
      refreshMarketplaces();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.mpMark = async (mid) => {
    const s = mpSubmissions.find((x) => x.marketplace_id === mid);
    try {
      await invoke2("mark_submission", { productId: mpProductId, marketplaceId: mid, kind: s?.kind || "mcp", status: "listed", resultUrl: null });
      showToast("\u5DF2\u6807\u8BB0\u4E0A\u67B6", "success");
      refreshMarketplaces();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.unblockTask = async (id) => {
    try {
      await invoke2("unblock_task", { id });
      showToast("\u5DF2\u89E3\u9664\u963B\u585E\uFF0C\u5C06\u91CD\u65B0\u6267\u884C", "success");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.cancelTask = async (id) => {
    try {
      await invoke2("cancel_task", { id });
      showToast("\u5DF2\u53D6\u6D88", "success");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.retryTask = async (id) => {
    try {
      await invoke2("retry_task", { id });
      showToast("\u5DF2\u91CD\u65B0\u5165\u961F", "success");
      refreshTasksPage();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  function renderTasks() {
    const container = document.getElementById("tasksList");
    if (!container) return;
    const pending = tasks.filter((t2) => t2.status === "pending").length;
    const running = tasks.filter((t2) => t2.status === "running").length;
    const completed = tasks.filter((t2) => t2.status === "completed").length;
    const failed = tasks.filter((t2) => t2.status === "failed").length;
    const statPending = document.getElementById("taskStatPending");
    const statRunning = document.getElementById("taskStatRunning");
    const statCompleted = document.getElementById("taskStatCompleted");
    if (statPending) statPending.textContent = `${pending} pending`;
    if (statRunning) statRunning.textContent = `${running} running`;
    if (statCompleted) statCompleted.textContent = `${completed + failed} done`;
    if (tasks.length === 0) {
      container.innerHTML = `
      <div class="tasks-empty">
        <div class="tasks-empty-icon">\u{1F4CB}</div>
        <p>No tasks yet</p>
        <p class="text-muted">Tasks will appear here when you publish or reply</p>
      </div>
    `;
      return;
    }
    container.innerHTML = tasks.map((task) => {
      const statusIcon = {
        pending: "\u23F3",
        running: "\u26A1",
        completed: "\u2705",
        failed: "\u274C"
      }[task.status];
      const progressPercent = task.total > 0 ? Math.round(task.progress / task.total * 100) : 0;
      const timeAgo = formatTimeAgo(task.createdAt);
      let resultSummary = "";
      if (task.status === "completed" && task.data?.results) {
        const successCount = task.data.successCount || 0;
        const failCount = task.data.failCount || 0;
        resultSummary = `
        <div class="task-results">
          <span class="result-success">${successCount} success</span>
          ${failCount > 0 ? `<span class="result-fail">${failCount} failed</span>` : ""}
        </div>
      `;
      }
      let completionInfo = "";
      if (task.completedAt) {
        const duration = Math.round((task.completedAt.getTime() - task.createdAt.getTime()) / 1e3);
        completionInfo = `<span>in ${formatDuration(duration)}</span>`;
      }
      return `
      <div class="task-item ${task.status}">
        <div class="task-status ${task.status}">${statusIcon}</div>
        <div class="task-info">
          <div class="task-title">${escapeHtml(task.title)}</div>
          <div class="task-meta">
            <span class="task-type-badge">${task.type}</span>
            <span>${timeAgo}</span>
            ${completionInfo}
            ${task.error ? `<span class="text-error">${escapeHtml(task.error)}</span>` : ""}
          </div>
          ${resultSummary}
        </div>
        ${task.status === "running" || task.status === "pending" ? `
          <div class="task-progress">
            <div class="task-progress-bar">
              <div class="task-progress-fill" style="width: ${progressPercent}%"></div>
            </div>
            <div class="task-progress-text">${task.progress}/${task.total}</div>
          </div>
        ` : ""}
        <div class="task-actions">
          ${task.status === "completed" && task.data?.results ? `<button class="btn btn-small btn-secondary" onclick="viewTaskResults('${task.id}')">Details</button>` : ""}
          ${task.status === "failed" ? `<button class="btn btn-small btn-secondary" onclick="retryTask('${task.id}')">Retry</button>` : ""}
          ${task.status !== "running" ? `<button class="btn btn-small btn-secondary" onclick="removeTask('${task.id}')">Remove</button>` : ""}
        </div>
      </div>
    `;
    }).join("");
  }
  function formatDuration(seconds) {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor(seconds % 3600 / 60)}m`;
  }
  function viewTaskResults(taskId) {
    const task = tasks.find((t2) => t2.id === taskId);
    if (!task?.data?.results) return;
    const results = task.data.results;
    const html = `
    <div class="modal active" id="taskResultsModal">
      <div class="modal-content">
        <div class="modal-header">
          <h3>Task Results</h3>
          <button class="modal-close" onclick="closeModal('taskResultsModal')">&times;</button>
        </div>
        <div class="modal-body">
          <div class="results-summary">
            <span class="result-success-badge">${task.data.successCount || 0} Success</span>
            ${task.data.failCount > 0 ? `<span class="result-fail-badge">${task.data.failCount} Failed</span>` : ""}
          </div>
          <div class="results-list">
            ${results.map((r) => `
              <div class="result-item ${r.success ? "success" : "failed"}">
                <span class="result-platform">${PLATFORM_ICONS[r.platform] || "\u{1F4C4}"} ${r.platform}</span>
                <span class="result-status">${r.success ? "\u2705" : "\u274C"}</span>
                ${r.url ? `<a href="${r.url}" target="_blank" class="result-link">View</a>` : ""}
                ${r.error ? `<span class="result-error">${r.error}</span>` : ""}
                ${r.retried ? '<span class="result-retry-badge">retried</span>' : ""}
              </div>
            `).join("")}
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-primary" onclick="closeModal('taskResultsModal')">Close</button>
        </div>
      </div>
    </div>
  `;
    document.getElementById("taskResultsModal")?.remove();
    document.body.insertAdjacentHTML("beforeend", html);
  }
  window.viewTaskResults = viewTaskResults;
  function clearCompletedTasks() {
    tasks = tasks.filter((t2) => t2.status !== "completed" && t2.status !== "failed");
    renderTasks();
    showToast(t("msg.clearedTasks"), "success");
  }
  window.retryTask = function(taskId) {
    const task = tasks.find((t2) => t2.id === taskId);
    if (task && task.status === "failed") {
      task.status = "pending";
      task.progress = 0;
      task.error = void 0;
      renderTasks();
      processTaskQueue();
    }
  };
  window.removeTask = function(taskId) {
    tasks = tasks.filter((t2) => t2.id !== taskId);
    renderTasks();
  };
  function queuePublishTask() {
    if (generatedContents.length === 0) {
      showToast(t("msg.noContentToPublish"), "error");
      return;
    }
    const productNames = [...new Set(generatedContents.map((c) => c.product_name || "Product"))];
    const title = productNames.length > 1 ? `Publish ${generatedContents.length} posts for ${productNames.length} products` : `Publish ${generatedContents.length} posts for ${productNames[0]}`;
    createTask("publish", title, { contents: generatedContents });
    showToast("Task added to queue", "success");
    navigateTo("tasks");
  }
  var postWired = false;
  function postFieldVal(id) {
    return document.getElementById(id)?.value?.trim() || "";
  }
  function postSetVal(id, v) {
    const el = document.getElementById(id);
    if (el) el.value = v;
  }
  function postLocalToUtc(local) {
    if (!local) return void 0;
    const d = new Date(local);
    if (isNaN(d.getTime())) return void 0;
    return d.toISOString();
  }
  function postCollect() {
    const editId = postFieldVal("postEditId");
    const topics = postFieldVal("postTopics").split(",").map((s) => s.trim()).filter(Boolean);
    const media = postFieldVal("postMedia").split(",").map((s) => s.trim()).filter(Boolean);
    const account_id = document.getElementById("postAccount")?.value || void 0;
    const scheduled_at = postLocalToUtc(postFieldVal("postSchedule"));
    return {
      id: editId || void 0,
      product_id: document.getElementById("postProduct")?.value || void 0,
      platform: document.getElementById("postPlatform")?.value || "twitter",
      account_id,
      title: postFieldVal("postTitle") || void 0,
      body: postFieldVal("postBody"),
      topics,
      media_paths: media,
      scheduled_at
    };
  }
  async function postPopulateAccounts() {
    const platform = document.getElementById("postPlatform")?.value || "twitter";
    const sel = document.getElementById("postAccount");
    if (!sel) return;
    let list = [];
    try {
      list = await invoke2("list_accounts") || [];
    } catch {
      list = [];
    }
    const alias = (p) => (p === "x" ? "twitter" : p).toLowerCase();
    const matched = list.filter((a) => alias(a.platform || "") === alias(platform));
    const pool = matched.length ? matched : list;
    sel.innerHTML = `<option value="">\uFF08\u7EE7\u627F\u5168\u5C40\u9ED8\u8BA4 profile\uFF09</option>` + pool.map((a) => `<option value="${a.id}">${escapeHtml(a.username || a.name || a.id)} \xB7 ${escapeHtml(a.platform || "")}</option>`).join("");
  }
  async function loadContentPage() {
    if (!postWired) {
      postWired = true;
      document.getElementById("btnPostRefresh")?.addEventListener("click", () => refreshPosts());
      document.getElementById("postPlatform")?.addEventListener("change", () => postPopulateAccounts());
      document.getElementById("btnPostClear")?.addEventListener("click", () => {
        ["postEditId", "postTitle", "postBody", "postTopics", "postMedia", "postSchedule"].forEach((i) => postSetVal(i, ""));
        showToast("\u5DF2\u6E05\u7A7A", "success");
      });
      document.getElementById("btnPostGen")?.addEventListener("click", async () => {
        const product_id = document.getElementById("postProduct")?.value;
        const platform = document.getElementById("postPlatform")?.value;
        if (!product_id) {
          showToast("\u8BF7\u5148\u9009\u4EA7\u54C1", "error");
          return;
        }
        const lang = platform === "xiaohongshu" || platform === "douyin" ? "zh" : "en";
        showToast("AI \u751F\u6210\u4E2D\u2026", "info");
        try {
          const g = await invoke2("generate_post_content", { productId: product_id, platform, language: lang });
          if (g.title && !postFieldVal("postTitle")) postSetVal("postTitle", g.title);
          postSetVal("postBody", g.body || "");
          postSetVal("postTopics", (g.topics || []).join(", "));
          showToast("\u6587\u6848\u5DF2\u751F\u6210", "success");
        } catch (e) {
          showToast("\u751F\u6210\u5931\u8D25: " + e, "error");
        }
      });
      document.getElementById("btnPostSave")?.addEventListener("click", async () => {
        const p = postCollect();
        if (!p.body && !p.media_paths.length) {
          showToast("\u6B63\u6587\u6216\u5A92\u4F53\u81F3\u5C11\u8981\u6709\u4E00\u4E2A", "error");
          return;
        }
        try {
          const id = await invoke2("save_post", { post: p });
          postSetVal("postEditId", id);
          showToast("\u8349\u7A3F\u5DF2\u4FDD\u5B58", "success");
          refreshPosts();
        } catch (e) {
          showToast("\u4FDD\u5B58\u5931\u8D25: " + e, "error");
        }
      });
      document.getElementById("btnPostSchedule")?.addEventListener("click", async () => {
        const p = postCollect();
        if (!p.scheduled_at) {
          showToast("\u8BF7\u5148\u9009\u5B9A\u65F6\u65F6\u95F4", "error");
          return;
        }
        if (!p.body && !p.media_paths.length) {
          showToast("\u6B63\u6587\u6216\u5A92\u4F53\u81F3\u5C11\u8981\u6709\u4E00\u4E2A", "error");
          return;
        }
        try {
          const id = await invoke2("save_post", { post: p });
          await invoke2("schedule_post", { id, scheduledAt: p.scheduled_at });
          postSetVal("postEditId", id);
          showToast("\u5DF2\u6392\u671F\uFF0C\u5230\u70B9\u81EA\u52A8\u53D1\u5E03", "success");
          refreshPosts();
        } catch (e) {
          showToast("\u6392\u671F\u5931\u8D25: " + e, "error");
        }
      });
      document.getElementById("btnPostNow")?.addEventListener("click", async () => {
        const p = postCollect();
        if (!p.body && !p.media_paths.length) {
          showToast("\u6B63\u6587\u6216\u5A92\u4F53\u81F3\u5C11\u8981\u6709\u4E00\u4E2A", "error");
          return;
        }
        try {
          const id = await invoke2("save_post", { post: { ...p, scheduled_at: void 0 } });
          await invoke2("publish_post_now", { id });
          postSetVal("postEditId", id);
          showToast("\u5DF2\u5165\u961F\uFF0C\u5F15\u64CE\u4E0B\u4E00\u62CD\u81EA\u52A8\u53D1\u5E03", "success");
          refreshPosts();
        } catch (e) {
          showToast("\u53D1\u5E03\u5931\u8D25: " + e, "error");
        }
      });
      document.getElementById("btnPostImage")?.addEventListener("click", () => aiGenerateMedia("image"));
      document.getElementById("btnPostVideo")?.addEventListener("click", () => aiGenerateMedia("video"));
      document.getElementById("btnPostMatrix")?.addEventListener("click", () => openMatrixModal());
      document.getElementById("btnPostCalendar")?.addEventListener("click", () => toggleCalendar());
    }
    try {
      const products2 = await invoke2("list_products");
      const sel = document.getElementById("postProduct");
      if (sel) sel.innerHTML = (products2 || []).map((p) => `<option value="${p.id}">${escapeHtml(p.name)}</option>`).join("");
    } catch (e) {
      console.error(e);
    }
    await postPopulateAccounts();
    refreshPosts();
  }
  var POST_STATUS_LABEL = {
    draft: "\u8349\u7A3F",
    scheduled: "\u23F0\u5DF2\u6392\u671F",
    publishing: "\u{1F504}\u53D1\u5E03\u4E2D",
    published: "\u2705\u5DF2\u53D1\u5E03",
    failed: "\u274C\u5931\u8D25",
    canceled: "\u5DF2\u53D6\u6D88"
  };
  async function refreshPosts() {
    const c = document.getElementById("postList");
    if (!c) return;
    let posts = [];
    try {
      posts = await invoke2("list_posts") || [];
    } catch (e) {
      posts = [];
    }
    if (!posts.length) {
      c.innerHTML = '<p class="text-muted" style="padding:12px;">\u8FD8\u6CA1\u6709\u5185\u5BB9\u3002\u4E0A\u9762\u5199\u4E00\u6761\uFF0C\u5B58\u8349\u7A3F / \u5B9A\u65F6 / \u7ACB\u5373\u53D1\u5E03\u3002</p>';
      return;
    }
    c.innerHTML = `<div class="card" style="padding:8px;">${posts.map(postRow).join("")}</div>`;
  }
  function postRow(p) {
    const when = p.scheduled_at ? new Date(p.scheduled_at).toLocaleString() : "";
    const sub = p.status === "scheduled" && when ? `\u23F0 ${when}` : p.published_at ? `\u53D1\u5E03\u4E8E ${new Date(p.published_at).toLocaleString()}` : p.error ? `<span style="color:#e55">${escapeHtml(p.error.slice(0, 80))}</span>` : "";
    const preview = escapeHtml((p.title ? p.title + " \u2014 " : "") + (p.body || "").slice(0, 90));
    const mediaTag = p.media_type !== "none" ? `<span class="badge">${p.media_type === "video" ? "\u{1F3AC}\u89C6\u9891" : "\u{1F5BC}\u56FE\u6587"}\xD7${p.media_paths.length}</span>` : "";
    const actions = [];
    if (p.status === "draft" || p.status === "failed") actions.push(`<button class="btn btn-small btn-primary" onclick="postPublishNow('${p.id}')">\u{1F680}\u53D1\u5E03</button>`);
    if (p.status === "scheduled" || p.status === "publishing") actions.push(`<button class="btn btn-small btn-secondary" onclick="postCancel('${p.id}')">\u53D6\u6D88</button>`);
    if (p.result_url && p.status === "published") actions.push(`<a class="btn btn-small btn-secondary" href="${escapeHtml(p.result_url)}" target="_blank">\u6253\u5F00\u2197</a>`);
    actions.push(`<button class="btn btn-small btn-secondary" onclick="postEdit('${p.id}')">\u7F16\u8F91</button>`);
    actions.push(`<button class="btn btn-small btn-secondary" onclick="postDelete('${p.id}')">\u5220</button>`);
    return `<div class="task-item" style="padding:8px 10px;border-bottom:1px solid var(--border,#eee);">
    <div style="display:flex;justify-content:space-between;align-items:center;gap:8px;flex-wrap:wrap;">
      <div style="min-width:0;flex:1;">
        <div style="font-size:13px;">${preview}</div>
        <div class="text-muted" style="font-size:11px;margin-top:2px;">
          <span class="badge">${escapeHtml(p.platform)}</span> ${mediaTag}
          <strong>${POST_STATUS_LABEL[p.status] || p.status}</strong> ${sub}
        </div>
      </div>
      <div class="btn-group" style="gap:4px;flex-wrap:wrap;">${actions.join("")}</div>
    </div>
  </div>`;
  }
  window.postPublishNow = async (id) => {
    try {
      await invoke2("publish_post_now", { id });
      showToast("\u5DF2\u5165\u961F\u53D1\u5E03", "success");
      refreshPosts();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.postCancel = async (id) => {
    try {
      await invoke2("cancel_post", { id });
      showToast("\u5DF2\u53D6\u6D88", "success");
      refreshPosts();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.postDelete = async (id) => {
    try {
      await invoke2("delete_post", { id });
      showToast("\u5DF2\u5220\u9664", "success");
      refreshPosts();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.postEdit = async (id) => {
    let posts = [];
    try {
      posts = await invoke2("list_posts") || [];
    } catch {
    }
    const p = posts.find((x) => x.id === id);
    if (!p) return;
    postSetVal("postEditId", p.id);
    document.getElementById("postPlatform").value = p.platform === "x" ? "twitter" : p.platform;
    await postPopulateAccounts();
    if (p.account_id) document.getElementById("postAccount").value = p.account_id;
    if (p.product_id) document.getElementById("postProduct").value = p.product_id;
    postSetVal("postTitle", p.title || "");
    postSetVal("postBody", p.body || "");
    postSetVal("postTopics", (p.topics || []).join(", "));
    postSetVal("postMedia", (p.media_paths || []).join(", "));
    if (p.scheduled_at) {
      const d = new Date(p.scheduled_at);
      const pad = (n) => String(n).padStart(2, "0");
      postSetVal("postSchedule", `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`);
    }
    showToast("\u5DF2\u8F7D\u5165\u5230\u7F16\u8F91\u533A", "success");
    window.scrollTo({ top: 0, behavior: "smooth" });
  };
  function aiMediaPrompt() {
    const product = document.getElementById("postProduct")?.selectedOptions?.[0]?.text || "";
    const title = postFieldVal("postTitle");
    const body = postFieldVal("postBody");
    const base = [title, body].filter(Boolean).join("\u3002").slice(0, 400);
    return base || `\u4E3A\u4EA7\u54C1\u300C${product}\u300D\u751F\u6210\u4E00\u5F20\u9AD8\u8D28\u611F\u3001\u73B0\u4EE3\u3001\u5E72\u51C0\u7684\u8425\u9500\u914D\u56FE`;
  }
  async function aiGenerateMedia(kind) {
    const hint = document.getElementById("aiMediaHint");
    const prompt2 = aiMediaPrompt();
    if (!prompt2) {
      showToast("\u5148\u5199\u70B9\u6B63\u6587/\u6807\u9898\uFF0CAI \u636E\u6B64\u914D\u56FE", "error");
      return;
    }
    const setHint = (t2) => {
      if (hint) hint.textContent = t2;
    };
    try {
      let path;
      if (kind === "image") {
        const ar = document.getElementById("postImageAR")?.value || void 0;
        setHint("\u{1F5BC} \u914D\u56FE\u751F\u6210\u4E2D\u2026\u7EA6 10-20 \u79D2");
        showToast("AI \u914D\u56FE\u751F\u6210\u4E2D\u2026", "info");
        path = await invoke2("generate_ai_image", { prompt: prompt2, aspectRatio: ar });
      } else {
        setHint("\u{1F3AC} \u89C6\u9891\u751F\u6210\u4E2D\u2026\u7EA6 1-3 \u5206\u949F\uFF0C\u8BF7\u52FF\u5173\u95ED");
        showToast("AI \u89C6\u9891\u751F\u6210\u4E2D\uFF081-3 \u5206\u949F\uFF09\u2026", "info");
        const plat = document.getElementById("postPlatform")?.value;
        const ar = plat === "douyin" || plat === "xiaohongshu" ? "9:16" : "16:9";
        path = await invoke2("generate_ai_video", { prompt: prompt2, model: null, aspectRatio: ar });
      }
      const cur = postFieldVal("postMedia");
      postSetVal("postMedia", cur ? `${cur}, ${path}` : path);
      setHint(`\u2705 \u5DF2\u751F\u6210\u5E76\u52A0\u5165\u5A92\u4F53\uFF1A${path.split(/[\\/]/).pop()}`);
      showToast(kind === "image" ? "\u914D\u56FE\u5DF2\u751F\u6210" : "\u89C6\u9891\u5DF2\u751F\u6210", "success");
    } catch (e) {
      setHint("");
      showToast((kind === "image" ? "\u914D\u56FE\u5931\u8D25: " : "\u89C6\u9891\u5931\u8D25: ") + e, "error");
    }
  }
  var calendarMonth = /* @__PURE__ */ new Date();
  async function toggleCalendar() {
    const el = document.getElementById("postCalendar");
    const list = document.getElementById("postList");
    if (!el) return;
    if (el.style.display === "none") {
      el.style.display = "block";
      if (list) list.style.display = "none";
      await renderCalendar();
    } else {
      el.style.display = "none";
      if (list) list.style.display = "block";
    }
  }
  async function renderCalendar() {
    const el = document.getElementById("postCalendar");
    if (!el) return;
    let posts = [];
    try {
      posts = await invoke2("list_posts") || [];
    } catch {
    }
    const byDay = {};
    for (const p of posts) {
      const when = p.scheduled_at || p.published_at;
      if (!when) continue;
      const d = new Date(when);
      if (isNaN(d.getTime())) continue;
      const key = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      (byDay[key] ||= []).push(p);
    }
    const y = calendarMonth.getFullYear(), m = calendarMonth.getMonth();
    const first = new Date(y, m, 1);
    const startDow = first.getDay();
    const daysInMonth = new Date(y, m + 1, 0).getDate();
    const today = /* @__PURE__ */ new Date();
    const monthLabel = `${y} \u5E74 ${m + 1} \u6708`;
    const dows = ["\u65E5", "\u4E00", "\u4E8C", "\u4E09", "\u56DB", "\u4E94", "\u516D"];
    let cells = "";
    for (let i = 0; i < startDow; i++) cells += `<div></div>`;
    const platEmoji = { twitter: "\u{1D54F}", x: "\u{1D54F}", linkedin: "\u{1F4BC}", reddit: "\u{1F47D}", xiaohongshu: "\u{1F4D5}", douyin: "\u{1F3B5}" };
    for (let day = 1; day <= daysInMonth; day++) {
      const key = `${y}-${m}-${day}`;
      const items = byDay[key] || [];
      const isToday = today.getFullYear() === y && today.getMonth() === m && today.getDate() === day;
      const chips = items.slice(0, 4).map((p) => {
        const t2 = new Date(p.scheduled_at || p.published_at);
        const hh = String(t2.getHours()).padStart(2, "0") + ":" + String(t2.getMinutes()).padStart(2, "0");
        const st = p.status === "published" ? "\u2705" : p.status === "failed" ? "\u274C" : "\u23F0";
        const acct = p.account_id ? "" : "";
        return `<div title="${escapeHtml((p.title || "") + " " + (p.body || "").slice(0, 60))}" style="font-size:10px;background:var(--bg-soft,#f2f3f7);border-radius:4px;padding:1px 4px;margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;cursor:pointer;" onclick="postEdit('${p.id}')">${st}${platEmoji[p.platform] || ""} ${hh} ${escapeHtml((p.title || p.body || "").slice(0, 10))}${acct}</div>`;
      }).join("");
      const more = items.length > 4 ? `<div style="font-size:10px;color:var(--text-muted,#888);">+${items.length - 4}\u2026</div>` : "";
      cells += `<div style="border:1px solid var(--border,#eee);border-radius:6px;min-height:74px;padding:4px;${isToday ? "outline:2px solid var(--accent,#6c5ce7);" : ""}">
      <div style="font-size:11px;color:${isToday ? "var(--accent,#6c5ce7)" : "var(--text-muted,#999)"};font-weight:${isToday ? "700" : "400"};">${day}</div>
      ${chips}${more}
    </div>`;
    }
    el.innerHTML = `<div class="card" style="padding:14px;">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;">
      <button class="btn btn-small btn-secondary" onclick="calMove(-1)">\u2039 \u4E0A\u6708</button>
      <strong>${monthLabel} \xB7 \u5185\u5BB9\u65E5\u5386</strong>
      <button class="btn btn-small btn-secondary" onclick="calMove(1)">\u4E0B\u6708 \u203A</button>
    </div>
    <div style="display:grid;grid-template-columns:repeat(7,1fr);gap:4px;margin-bottom:4px;">
      ${dows.map((d) => `<div style="text-align:center;font-size:11px;color:var(--text-muted,#999);">${d}</div>`).join("")}
    </div>
    <div style="display:grid;grid-template-columns:repeat(7,1fr);gap:4px;">${cells}</div>
    <div class="text-muted" style="font-size:11px;margin-top:8px;">\u23F0\u5F85\u53D1 \xB7 \u2705\u5DF2\u53D1 \xB7 \u274C\u5931\u8D25 \xB7 \u70B9\u683C\u5B50\u91CC\u7684\u6761\u76EE\u53EF\u8F7D\u5165\u7F16\u8F91\u3002\u6392\u671F\u5728\u300C\u5B9A\u65F6\u53D1\u5E03\u300D\u91CC\u8BBE\u3002</div>
  </div>`;
  }
  window.calMove = (delta) => {
    calendarMonth.setMonth(calendarMonth.getMonth() + delta);
    renderCalendar();
  };
  var factoryItems = [];
  async function openMatrixModal() {
    const product_id = document.getElementById("postProduct")?.value;
    if (!product_id) {
      showToast("\u8BF7\u5148\u5728\u4E0A\u65B9\u9009\u4EA7\u54C1", "error");
      return;
    }
    factoryItems = [];
    let accts = [];
    try {
      accts = await invoke2("list_accounts") || [];
    } catch {
    }
    accts = accts.filter((a) => (a.status || "active") === "active");
    let overlay = document.getElementById("matrixOverlay");
    if (overlay) overlay.remove();
    overlay = document.createElement("div");
    overlay.id = "matrixOverlay";
    overlay.style.cssText = "position:fixed;inset:0;background:rgba(0,0,0,.55);z-index:9999;display:flex;align-items:center;justify-content:center;";
    const slotRows = accts.length ? accts.map((a, i) => `<label style="display:flex;align-items:center;gap:6px;font-size:12px;padding:3px 0;">
        <input type="checkbox" class="facSlot" data-idx="${i}" value="${a.id}" checked
          data-platform="${escapeHtml(a.platform || "")}" data-email="${escapeHtml(a.persona_email || a.email || "")}" />
        <span class="badge">${escapeHtml(a.platform || "?")}</span> ${escapeHtml(a.persona_email || a.email || a.username || a.id)}
      </label>`).join("") : `<div class="text-muted" style="font-size:12px;">\u8FD8\u6CA1\u6709\u6D3B\u8DC3\u8D26\u53F7\u3002\u5148\u5230\u300C\u90AE\u7BB1\u8D26\u53F7\u300D\u5F00\u901A\u8D26\u53F7\uFF0C\u518D\u6765\u77E9\u9635\u5DE5\u5382\u3002</div>`;
    overlay.innerHTML = `<div class="card" style="width:min(820px,94vw);max-height:90vh;overflow:auto;padding:18px;">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
      <h3 style="margin:0;">\u{1F3ED} \u77E9\u9635\u5185\u5BB9\u5DE5\u5382</h3>
      <button class="btn btn-small btn-secondary" onclick="closeMatrix()">\u2715</button>
    </div>
    <p class="text-muted" style="font-size:12px;margin:0 0 12px;">\u4E00\u4E2A\u521B\u610F \u2192\uFF08\u6BCF\u4E2A\u90AE\u7BB1\u4E00\u6761\uFF09<strong>\u9010\u5E73\u53F0\u6B63\u786E\u5F62\u6001 + \u4E0D\u540C\u4EBA\u8BBE\u53E3\u543B</strong>\u7684\u6210\u54C1\u6587\u6848\uFF0C\u6309\u9700\u914D\u56FE\uFF0C\u9519\u5CF0\u94FA\u5230\u5404 profile\uFF0C\u5F15\u64CE\u5230\u70B9\u81EA\u52A8\u53D1\u3002</p>
    <div style="margin-bottom:10px;">
      <label class="text-muted" style="font-size:12px;">\u521B\u610F / \u4E3B\u9898\uFF08\u7559\u7A7A=\u81EA\u52A8\u56F4\u7ED5\u4EA7\u54C1\u5356\u70B9\uFF09</label>
      <textarea id="facIdea" rows="2" placeholder="\u4F8B\uFF1A\u7528\u4E00\u4E2A\u771F\u5B9E\u573A\u666F\u8BF4\u660E\u8FD9\u5DE5\u5177\u600E\u4E48\u5E2E\u4EBA\u7701\u65F6\u95F4" style="width:100%;padding:8px;resize:vertical;"></textarea>
    </div>
    <div style="display:flex;gap:14px;flex-wrap:wrap;">
      <div style="flex:1;min-width:240px;">
        <div style="font-size:12px;font-weight:600;margin-bottom:4px;">\u6295\u653E\u69FD\u4F4D\uFF08\u6BCF\u4E2A=\u4E00\u4E2A\u90AE\u7BB1\u53D1\u4E00\u6761\uFF0C${accts.length}\uFF09
          <a href="#" style="font-size:11px;font-weight:400;margin-left:6px;" onclick="facToggleAll(true);return false;">\u5168\u9009</a> \xB7
          <a href="#" style="font-size:11px;font-weight:400;" onclick="facToggleAll(false);return false;">\u5168\u4E0D\u9009</a>
        </div>
        <div style="max-height:180px;overflow:auto;border:1px solid var(--border,#eee);border-radius:6px;padding:8px;">${slotRows}</div>
      </div>
    </div>
    <div class="btn-group" style="margin-top:10px;">
      <button class="btn btn-primary" id="facGenBtn" onclick="factoryGenerate('${product_id}')">\u2728 \u751F\u6210\u6210\u54C1\uFF08\u9010\u5E73\u53F0\xD7\u9010\u4EBA\u8BBE\uFF09</button>
    </div>
    <div id="facPreview" style="margin-top:12px;"></div>
    <div id="facCommit" style="display:none;border-top:1px solid var(--border,#eee);padding-top:10px;margin-top:12px;">
      <div class="btn-group" style="margin-bottom:8px;">
        <button class="btn btn-secondary btn-small" onclick="factoryImageAll('${product_id}')">\u{1F5BC} \u7ED9\u5168\u90E8\u9700\u8981\u7684\u914D\u56FE</button>
      </div>
      <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;">
        <label style="font-size:12px;">\u5F00\u59CB\u65F6\u95F4 <input id="facStart" type="datetime-local" style="padding:4px;" /></label>
        <label style="font-size:12px;">\u6BCF\u6761\u95F4\u9694 <input id="facInterval" type="number" value="45" min="0" style="width:56px;padding:4px;" /> \u5206\u949F</label>
      </div>
      <div class="text-muted" style="font-size:11px;margin-top:4px;">\u7559\u7A7A\u5F00\u59CB\u65F6\u95F4=\u5168\u90E8\u5B58\u8349\u7A3F\uFF1B\u586B\u4E86=\u4ECE\u8BE5\u65F6\u95F4\u8D77\u6BCF\u9694 N \u5206\u949F\u81EA\u52A8\u53D1\u4E00\u6761\uFF08\u9519\u5CF0\u9632\u5173\u8054\uFF09\u3002</div>
      <div class="btn-group" style="margin-top:10px;">
        <button class="btn btn-primary" onclick="factoryCommit()">\u{1F680} \u94FA\u91CF\uFF08\u5EFA\u6210 ${accts.length ? "" : ""}\u5B9A\u65F6\u5E16\uFF09</button>
        <button class="btn btn-secondary" onclick="closeMatrix()">\u53D6\u6D88</button>
      </div>
    </div>
  </div>`;
    document.body.appendChild(overlay);
  }
  window.closeMatrix = () => {
    document.getElementById("matrixOverlay")?.remove();
  };
  window.facToggleAll = (on) => {
    document.querySelectorAll(".facSlot").forEach((e) => e.checked = on);
  };
  window.factoryGenerate = async (productId) => {
    const idea = document.getElementById("facIdea")?.value || "";
    const slots = Array.from(document.querySelectorAll(".facSlot:checked")).map((e) => {
      const el = e;
      return { account_id: el.value, platform: el.dataset.platform || "twitter", persona_email: el.dataset.email || "" };
    });
    if (!slots.length) {
      showToast("\u8BF7\u81F3\u5C11\u9009\u4E00\u4E2A\u6295\u653E\u69FD\u4F4D", "error");
      return;
    }
    const btn = document.getElementById("facGenBtn");
    const prev = document.getElementById("facPreview");
    if (btn) {
      btn.disabled = true;
      btn.textContent = `\u2728 \u751F\u6210\u4E2D\u2026(${slots.length} \u6761\u9010\u6761\u4EA7\u51FA)`;
    }
    if (prev) prev.innerHTML = '<div class="text-muted" style="font-size:12px;">AI \u6B63\u5728\u4E3A\u6BCF\u4E2A\u5E73\u53F0/\u4EBA\u8BBE\u4EA7\u51FA\u6210\u54C1\u2026\u7EA6\u6BCF\u6761 3-6 \u79D2</div>';
    try {
      factoryItems = await invoke2("matrix_factory_generate", { productId, idea, items: slots });
      renderFactoryPreview();
      const commit = document.getElementById("facCommit");
      if (commit) commit.style.display = "block";
      showToast(`\u5DF2\u4EA7\u51FA ${factoryItems.length} \u6761\u6210\u54C1`, "success");
    } catch (e) {
      if (prev) prev.innerHTML = `<div style="color:#e55;font-size:12px;">\u751F\u6210\u5931\u8D25\uFF1A${escapeHtml("" + e)}</div>`;
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = "\u2728 \u91CD\u65B0\u751F\u6210\u6210\u54C1";
      }
    }
  };
  function renderFactoryPreview() {
    const prev = document.getElementById("facPreview");
    if (!prev) return;
    prev.innerHTML = factoryItems.map((it, i) => {
      const needImg = !!it.image_prompt && it.platform.toLowerCase() !== "linkedin" && it.platform.toLowerCase() !== "reddit";
      const hasImg = (it.media_paths || []).length > 0;
      const imgBtn = needImg ? hasImg ? `<span class="badge" style="background:#2ecc71;color:#fff;">\u{1F5BC}\u5DF2\u914D\u56FE</span>` : `<button class="btn btn-small btn-secondary" onclick="factoryImageOne(${i})">\u{1F5BC} \u914D\u56FE</button>` : "";
      return `<div style="border:1px solid var(--border,#eee);border-radius:8px;padding:10px;margin-bottom:8px;">
      <div style="display:flex;justify-content:space-between;align-items:center;gap:6px;flex-wrap:wrap;margin-bottom:6px;">
        <div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;">
          <span class="badge">${escapeHtml(it.platform)}</span>
          <span class="text-muted" style="font-size:11px;">${escapeHtml(it.persona_email || "")}</span>
          <span class="text-muted" style="font-size:11px;">\xB7 \u4EBA\u8BBE\uFF1A${escapeHtml(it.angle || "")}</span>
        </div>
        ${imgBtn}
      </div>
      ${["twitter", "x", "linkedin"].includes(it.platform.toLowerCase()) ? "" : `<input value="${escapeHtml(it.title || "")}" oninput="factoryEdit(${i},'title',this.value)" placeholder="\u6807\u9898" style="width:100%;padding:5px;margin-bottom:4px;font-size:12px;" />`}
      <textarea rows="4" oninput="factoryEdit(${i},'body',this.value)" style="width:100%;padding:6px;font-size:12px;resize:vertical;">${escapeHtml(it.body || "")}</textarea>
      ${(it.topics || []).length ? `<div style="font-size:11px;color:var(--accent,#6c5ce7);margin-top:3px;">${(it.topics || []).map((t2) => "#" + escapeHtml(t2)).join(" ")}</div>` : ""}
      ${hasImg ? `<div class="text-muted" style="font-size:11px;margin-top:3px;">\u{1F4CE} ${escapeHtml((it.media_paths[0] || "").split(/[\\/]/).pop())}</div>` : needImg && it.image_prompt ? `<div class="text-muted" style="font-size:11px;margin-top:3px;font-style:italic;">\u914D\u56FE\u5EFA\u8BAE\uFF1A${escapeHtml(it.image_prompt.slice(0, 70))}</div>` : ""}
    </div>`;
    }).join("");
  }
  window.factoryEdit = (i, field, val) => {
    if (factoryItems[i]) factoryItems[i][field] = val;
  };
  window.factoryImageOne = async (i) => {
    const it = factoryItems[i];
    if (!it) return;
    showToast(`\u7B2C${i + 1}\u6761\u914D\u56FE\u751F\u6210\u4E2D\u2026`, "info");
    try {
      const path = await invoke2("generate_ai_image", { prompt: it.image_prompt || it.body, aspectRatio: it.aspect_ratio || "1:1" });
      it.media_paths = [path];
      renderFactoryPreview();
      showToast(`\u7B2C${i + 1}\u6761\u5DF2\u914D\u56FE`, "success");
    } catch (e) {
      showToast("\u914D\u56FE\u5931\u8D25: " + e, "error");
    }
  };
  window.factoryImageAll = async () => {
    const targets = factoryItems.map((it, i) => ({ it, i })).filter(({ it }) => it.image_prompt && !(it.media_paths || []).length && it.platform.toLowerCase() !== "linkedin" && it.platform.toLowerCase() !== "reddit");
    if (!targets.length) {
      showToast("\u6CA1\u6709\u9700\u8981\u914D\u56FE\u7684\u6761\u76EE", "info");
      return;
    }
    showToast(`\u6B63\u5728\u4E3A ${targets.length} \u6761\u914D\u56FE\u2026`, "info");
    for (const { it, i } of targets) {
      try {
        const path = await invoke2("generate_ai_image", { prompt: it.image_prompt || it.body, aspectRatio: it.aspect_ratio || "1:1" });
        it.media_paths = [path];
        renderFactoryPreview();
      } catch (e) {
        showToast(`\u7B2C${i + 1}\u6761\u914D\u56FE\u5931\u8D25: ${e}`, "error");
      }
    }
    showToast("\u5168\u90E8\u914D\u56FE\u5B8C\u6210", "success");
  };
  window.factoryCommit = async () => {
    if (!factoryItems.length) {
      showToast("\u8BF7\u5148\u751F\u6210\u6210\u54C1", "error");
      return;
    }
    const startLocal = document.getElementById("facStart")?.value;
    const start_at = startLocal ? postLocalToUtc(startLocal) : void 0;
    const interval = parseInt(document.getElementById("facInterval")?.value || "0", 10);
    try {
      const n = await invoke2("factory_commit", { items: factoryItems, startAt: start_at, intervalMinutes: interval });
      showToast(`\u5DF2\u94FA\u91CF ${n} \u6761${start_at ? "\uFF08\u5DF2\u6392\u671F\uFF0C\u5F15\u64CE\u5230\u70B9\u81EA\u52A8\u53D1\uFF09" : "\uFF08\u8349\u7A3F\uFF09"}`, "success");
      window.closeMatrix();
      refreshPosts();
    } catch (e) {
      showToast("\u94FA\u91CF\u5931\u8D25: " + e, "error");
    }
  };
  var metricsWired = false;
  async function loadMetricsPage() {
    if (!metricsWired) {
      metricsWired = true;
      document.getElementById("btnMetricsRefresh")?.addEventListener("click", () => {
        refreshKeywords();
        refreshMetricsOverview();
        metricsLoadSettings();
      });
      document.getElementById("btnKwAdd")?.addEventListener("click", addKeyword);
      document.getElementById("btnMetricsCollect")?.addEventListener("click", collectMetricsNow);
      document.getElementById("metricsTrends")?.addEventListener("change", async (e) => {
        const on = e.target.checked;
        try {
          await invoke2("metrics_set_trends", { on });
          showToast(on ? "\u5DF2\u5F00\u542F Trends \u91C7\u96C6" : "\u5DF2\u5173\u95ED Trends \u91C7\u96C6", "success");
        } catch (err) {
          showToast("" + err, "error");
        }
      });
    }
    metricsLoadSettings();
    refreshKeywords();
    refreshMetricsOverview();
  }
  async function metricsLoadSettings() {
    try {
      const s = await invoke2("metrics_get_settings");
      const el = document.getElementById("metricsSettings");
      const last = s.last_tick ? new Date(s.last_tick).toLocaleString() : "\u5C1A\u672A\u91C7\u96C6";
      if (el) el.innerHTML = `\u91C7\u96C6 profile\uFF1A<b>${escapeHtml(s.profile)}</b> \xB7 \u5730\u533A\uFF1A<b>${escapeHtml(s.region)}</b> \xB7 \u4E0A\u6B21\u91C7\u96C6\uFF1A<b>${escapeHtml(last)}</b> \xB7 \u5F15\u64CE\u6BCF\u65E5\u81EA\u52A8\u8DD1\u4E00\u6B21`;
      const cb = document.getElementById("metricsTrends");
      if (cb) cb.checked = !!s.trends_on;
    } catch (e) {
    }
  }
  async function addKeyword() {
    const kw = document.getElementById("kwInput")?.value?.trim();
    const kind = document.getElementById("kwKind")?.value || "longtail";
    const domain = document.getElementById("kwDomain")?.value?.trim() || "doaipm.com";
    if (!kw) {
      showToast("\u8BF7\u8F93\u5165\u5173\u952E\u8BCD", "error");
      return;
    }
    try {
      await invoke2("metrics_add_keyword", { keyword: kw, kind, targetDomain: domain });
      document.getElementById("kwInput").value = "";
      showToast("\u5DF2\u6DFB\u52A0", "success");
      refreshKeywords();
    } catch (e) {
      showToast("" + e, "error");
    }
  }
  var KIND_LABEL = { brand: "\u54C1\u724C\u8BCD\xB7\u5B88", longtail: "\u957F\u5C3E\u8BCD\xB7\u653B", mention: "\u54C1\u724C\u63D0\u53CA" };
  async function refreshKeywords() {
    const box = document.getElementById("kwList");
    if (!box) return;
    let kws = [];
    try {
      kws = await invoke2("metrics_list_keywords") || [];
    } catch (e) {
      box.innerHTML = `<span class="text-muted">\u52A0\u8F7D\u5931\u8D25\uFF1A${escapeHtml("" + e)}</span>`;
      return;
    }
    if (!kws.length) {
      box.innerHTML = '<span class="text-muted" style="font-size:12px;">\u8FD8\u6CA1\u6709\u5173\u952E\u8BCD\uFF0C\u6DFB\u52A0\u51E0\u4E2A\u5F00\u59CB\u8FFD\u8E2A\u3002</span>';
      return;
    }
    const groups = ["brand", "longtail", "mention"];
    box.innerHTML = groups.map((g) => {
      const rows = kws.filter((k) => k.kind === g);
      if (!rows.length) return "";
      return `<div style="margin-bottom:8px;">
      <div class="text-muted" style="font-size:11px;margin-bottom:4px;">${KIND_LABEL[g] || g}</div>
      <div style="display:flex;flex-wrap:wrap;gap:6px;">
        ${rows.map((k) => `<span style="display:inline-flex;align-items:center;gap:6px;padding:4px 8px;border:1px solid var(--border,#ddd);border-radius:14px;font-size:12px;opacity:${k.enabled ? "1" : "0.45"};">
          <span title="\u76EE\u6807\u57DF\u540D\uFF1A${escapeHtml(k.target_domain)}">${escapeHtml(k.keyword)}</span>
          <a href="#" title="${k.enabled ? "\u505C\u7528" : "\u542F\u7528"}" onclick="metricsToggleKw('${k.id}',${k.enabled ? "false" : "true"});return false;" style="text-decoration:none;">${k.enabled ? "\u23F8" : "\u25B6"}</a>
          <a href="#" title="\u5220\u9664" onclick="metricsDeleteKw('${k.id}');return false;" style="text-decoration:none;color:#e55;">\u2715</a>
        </span>`).join("")}
      </div></div>`;
    }).join("");
  }
  window.metricsToggleKw = async (id, enabled) => {
    try {
      await invoke2("metrics_toggle_keyword", { id, enabled });
      refreshKeywords();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  window.metricsDeleteKw = async (id) => {
    try {
      await invoke2("metrics_delete_keyword", { id });
      showToast("\u5DF2\u5220\u9664", "success");
      refreshKeywords();
    } catch (e) {
      showToast("" + e, "error");
    }
  };
  async function collectMetricsNow() {
    const btn = document.getElementById("btnMetricsCollect");
    const orig = btn ? btn.innerText : "";
    if (btn) {
      btn.disabled = true;
      btn.innerText = "\u91C7\u96C6\u4E2D\u2026\uFF08\u7EA6 1 \u5206\u949F\uFF09";
    }
    try {
      const msg = await invoke2("metrics_collect_now");
      showToast(msg || "\u91C7\u96C6\u5B8C\u6210", "success");
      refreshMetricsOverview();
      metricsLoadSettings();
    } catch (e) {
      showToast("\u91C7\u96C6\u5931\u8D25\uFF1A" + e, "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.innerText = orig;
      }
    }
  }
  function sparkline(series, invert) {
    const vals = series.filter((v) => v != null);
    if (vals.length < 2) return '<span class="text-muted" style="font-size:11px;">\u6570\u636E\u70B9\u4E0D\u8DB3</span>';
    const w = 130, h = 30, pad = 3;
    let min = Math.min(...vals), max = Math.max(...vals);
    if (min === max) {
      min -= 1;
      max += 1;
    }
    const n = series.length;
    const pts = [];
    series.forEach((v, i) => {
      if (v == null) return;
      const x = pad + i * (w - 2 * pad) / (n - 1);
      let ny = (v - min) / (max - min);
      if (invert) ny = 1 - ny;
      const y = pad + (1 - ny) * (h - 2 * pad);
      pts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    });
    return `<svg width="${w}" height="${h}" style="vertical-align:middle;">
    <polyline points="${pts.join(" ")}" fill="none" stroke="#4a8cff" stroke-width="1.6" />
    ${pts.length ? `<circle cx="${pts[pts.length - 1].split(",")[0]}" cy="${pts[pts.length - 1].split(",")[1]}" r="2.4" fill="#4a8cff" />` : ""}
  </svg>`;
  }
  function rankText(v) {
    return v == null ? '<span class="text-muted">\u672A\u8FDB\u524D30</span>' : `#${v}`;
  }
  function deltaBadge(latest, previous, lowerBetter) {
    if (latest == null || previous == null) return "";
    const improved = lowerBetter ? latest < previous : latest > previous;
    const worse = lowerBetter ? latest > previous : latest < previous;
    const diff = Math.abs(latest - previous);
    if (diff === 0) return '<span class="text-muted" style="font-size:11px;">\u6301\u5E73</span>';
    const color = improved ? "#1a9d4a" : worse ? "#e55" : "var(--text-muted)";
    const arrow = improved ? "\u2191" : "\u2193";
    return `<span style="font-size:11px;color:${color};">${arrow}${diff}</span>`;
  }
  async function refreshMetricsOverview() {
    const box = document.getElementById("metricsOverview");
    if (!box) return;
    let rows = [];
    try {
      rows = await invoke2("metrics_overview") || [];
    } catch (e) {
      box.innerHTML = `<div class="card" style="padding:14px;"><span class="text-muted">\u52A0\u8F7D\u5931\u8D25\uFF1A${escapeHtml("" + e)}</span></div>`;
      return;
    }
    if (!rows.length) {
      box.innerHTML = `<div class="card" style="padding:18px;text-align:center;">
      <div style="font-size:13px;">\u8FD8\u6CA1\u6709\u4EFB\u4F55\u91C7\u96C6\u6570\u636E\u3002</div>
      <div class="text-muted" style="font-size:12px;margin-top:6px;">\u70B9\u53F3\u4E0A\u89D2\u300C\u{1F50D} \u7ACB\u5373\u91C7\u96C6\u300D\u8DD1\u7B2C\u4E00\u8F6E\uFF0C\u5EFA\u7ACB\u57FA\u7EBF\uFF08\u7EA6 1 \u5206\u949F\uFF09\u3002</div>
    </div>`;
      return;
    }
    const section = (title, hint, list, lowerBetter, fmt) => {
      if (!list.length) return "";
      return `<div class="card" style="margin:0 0 14px;padding:14px;">
      <h3 style="margin:0 0 4px;font-size:14px;">${title}</h3>
      <div class="text-muted" style="font-size:11px;margin-bottom:10px;">${hint}</div>
      <table style="width:100%;border-collapse:collapse;font-size:13px;">
        <thead><tr style="text-align:left;color:var(--text-muted);font-size:11px;">
          <th style="padding:4px 6px;">\u5173\u952E\u8BCD</th><th style="padding:4px 6px;">\u6700\u65B0</th><th style="padding:4px 6px;">\u53D8\u5316</th>
          <th style="padding:4px 6px;">\u8D8B\u52BF(\u8FD1${Math.max(...list.map((r) => r.series.length))}\u6B21)</th><th style="padding:4px 6px;">\u91C7\u6837</th><th style="padding:4px 6px;">\u66F4\u65B0</th>
        </tr></thead>
        <tbody>
        ${list.map((r) => `<tr style="border-top:1px solid var(--border,#eee);">
          <td style="padding:6px;">${escapeHtml(r.keyword)}</td>
          <td style="padding:6px;font-weight:600;">${fmt(r.latest)}</td>
          <td style="padding:6px;">${deltaBadge(r.latest, r.previous, lowerBetter)}</td>
          <td style="padding:6px;">${sparkline(r.series, lowerBetter)}</td>
          <td style="padding:6px;" class="text-muted">${r.samples}</td>
          <td class="text-muted" style="padding:6px;font-size:11px;">${r.captured_at ? escapeHtml(new Date(r.captured_at).toLocaleDateString()) : "\u2014"}</td>
        </tr>`).join("")}
        </tbody>
      </table>
    </div>`;
    };
    const brand = rows.filter((r) => r.source === "serp" && r.kind === "brand");
    const longtail = rows.filter((r) => r.source === "serp" && r.kind === "longtail");
    const mention = rows.filter((r) => r.source === "mention");
    const trends = rows.filter((r) => r.source === "trends");
    box.innerHTML = section("\u{1F6E1} \u54C1\u724C\u8BCD\u6392\u540D\uFF08\u5B88\uFF0C\u5E94\u6C38\u8FDC #1\uFF09", "\u81EA\u5DF1\u7684\u8BCD\u5FC5\u987B\u5B88\u4F4F\u3002\u6389\u51FA #1 = \u8B66\u62A5\u3002", brand, true, rankText) + section("\u2694\uFE0F \u957F\u5C3E\u8BCD\u6392\u540D\uFF08\u653B\uFF0C\u8D8A\u4F4E\u8D8A\u597D\uFF09", "\u60F3\u62A2\u7684\u5185\u5BB9\u8BCD\u3002\u4ECE\u300C\u672A\u8FDB\u524D30\u300D\u5F80 #1 \u722C\u5C31\u662F\u6210\u6548\u3002", longtail, true, rankText) + section("\u{1F4E3} \u54C1\u724C\u63D0\u53CA\uFF08\u9886\u5148\u6307\u6807\uFF0C\u8D8A\u591A\u8D8A\u597D\uFF09", "\u7B2C\u4E09\u65B9\u72EC\u7ACB\u57DF\u540D\u63D0\u5230\u4F60\u7684\u6570\u91CF\u3002\u6BD4 Trends \u65E9\u51E0\u4E2A\u6708\u53CD\u5E94\u3002", mention, false, (v) => v == null ? "0" : "" + v) + (trends.length ? section("\u{1F4CA} Google Trends\uFF08\u9ED8\u8BA4\u5173\uFF09", "\u5C0F\u54C1\u724C\u901A\u5E38\u65E0\u6570\u636E\uFF1B\u6709\u6570\u636E\u624D\u4F1A\u663E\u793A\u6570\u503C\u3002", trends, false, (v) => v == null ? "\u65E0\u6570\u636E" : "\u6709\u6570\u636E") : "");
  }
  var personasWired = false;
  async function loadPersonasPage() {
    if (!personasWired) {
      personasWired = true;
      document.getElementById("btnPersonaRefresh")?.addEventListener("click", () => {
        refreshAirport();
        refreshPersonas();
      });
      document.getElementById("btnAirportSave")?.addEventListener("click", saveAirport);
      document.getElementById("btnPersonaCreate")?.addEventListener("click", createPersona);
    }
    refreshAirport();
    refreshPersonas();
  }
  async function refreshAirport() {
    const el = document.getElementById("airportStatus");
    if (!el) return;
    try {
      const s = await invoke2("airport_status");
      if (!s.configured) {
        el.innerHTML = '<span style="color:#d97706;">\u8FD8\u6CA1\u914D\u7F6E\u673A\u573A\u8BA2\u9605</span> \u2014\u2014 \u5148\u5728\u4E0B\u9762\u7C98\u8D34\u4F60\u7684 Clash \u8BA2\u9605\u94FE\u63A5\uFF0C\u624D\u80FD\u7ED9\u8EAB\u4EFD\u5206\u914D\u51FA\u53E3 IP\u3002';
      } else {
        const regions = (s.by_region || []).slice(0, 8).map((r) => `${r[0]}\xD7${r[1]}`).join(" \xB7 ");
        el.innerHTML = `\u2705 \u8282\u70B9\u6C60\uFF1A<b>${s.total}</b> \u4E2A\uFF08\u7A7A\u95F2 <b>${s.free}</b> / \u5360\u7528 ${s.in_use}\uFF09\xB7 \u5185\u6838\u7AEF\u53E3 ${s.kernel_port}<br><span style="font-size:11px;">${escapeHtml(regions)}</span>`;
      }
    } catch (e) {
      el.innerHTML = `<span style="color:#e55;">\u8BFB\u53D6\u5931\u8D25\uFF1A${escapeHtml("" + e)}</span>`;
    }
  }
  async function saveAirport() {
    const url = document.getElementById("airportUrl")?.value?.trim();
    if (!url) {
      showToast("\u8BF7\u7C98\u8D34\u673A\u573A\u8BA2\u9605\u94FE\u63A5", "error");
      return;
    }
    const btn = document.getElementById("btnAirportSave");
    const orig = btn ? btn.innerText : "";
    if (btn) {
      btn.disabled = true;
      btn.innerText = "\u62C9\u53D6\u4E2D\u2026";
    }
    try {
      const msg = await invoke2("airport_set_subscription", { url });
      showToast(msg || "\u5DF2\u4FDD\u5B58", "success");
      document.getElementById("airportUrl").value = "";
      refreshAirport();
    } catch (e) {
      showToast("\u8BA2\u9605\u5931\u8D25\uFF1A" + e, "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.innerText = orig;
      }
    }
  }
  async function createPersona() {
    const email = document.getElementById("personaEmail")?.value?.trim();
    if (!email) {
      showToast("\u8BF7\u8F93\u5165\u4E00\u4E2A Gmail", "error");
      return;
    }
    const btn = document.getElementById("btnPersonaCreate");
    const orig = btn ? btn.innerText : "";
    if (btn) {
      btn.disabled = true;
      btn.innerText = "\u521B\u5EFA\u4E2D\u2026\uFF085-10\u79D2\uFF09";
    }
    try {
      await invoke2("persona_create", { email });
      document.getElementById("personaEmail").value = "";
      showToast("\u8EAB\u4EFD\u5DF2\u521B\u5EFA\uFF08\u72EC\u7ACB\u6D4F\u89C8\u5668+IP+\u6307\u7EB9\uFF09", "success");
      refreshPersonas();
      refreshAirport();
    } catch (e) {
      showToast("\u521B\u5EFA\u5931\u8D25\uFF1A" + e, "error");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.innerText = orig;
      }
    }
  }
  async function refreshPersonas() {
    const box = document.getElementById("personaList");
    if (!box) return;
    let rows = [];
    try {
      rows = await invoke2("persona_list") || [];
    } catch (e) {
      box.innerHTML = `<div class="card" style="padding:14px;"><span class="text-muted">\u52A0\u8F7D\u5931\u8D25\uFF1A${escapeHtml("" + e)}</span></div>`;
      return;
    }
    if (!rows.length) {
      box.innerHTML = `<div class="card" style="padding:18px;text-align:center;">
      <div style="font-size:13px;">\u8FD8\u6CA1\u6709\u8EAB\u4EFD\u3002</div>
      <div class="text-muted" style="font-size:12px;margin-top:6px;">\u4E0A\u9762\u586B\u4E2A Gmail \u70B9\u300C\u521B\u5EFA\u8EAB\u4EFD\u300D\uFF0C\u5C31\u6709\u4E86\u7B2C\u4E00\u5957\u72EC\u7ACB\u6D4F\u89C8\u5668+IP\u3002</div>
    </div>`;
      return;
    }
    box.innerHTML = rows.map((p) => `
    <div class="card" style="margin:0 0 8px;padding:12px 14px;display:flex;align-items:center;gap:12px;flex-wrap:wrap;">
      <div style="flex:1;min-width:200px;">
        <div style="font-weight:600;">${escapeHtml(p.email)}</div>
        <div class="text-muted" style="font-size:12px;margin-top:2px;">
          ${p.region ? escapeHtml(p.region) : "\u{1F310} \u8282\u70B9\u672A\u5206\u914D"}
          ${p.node_name ? `\xB7 <span title="${escapeHtml(p.node_name)}">${escapeHtml(p.node_name.slice(0, 18))}</span>` : ""}
          ${p.local_port ? `\xB7 \u7AEF\u53E3 ${p.local_port}` : ""}
          \xB7 \u8D26\u53F7 ${p.account_count}
        </div>
        <div id="ip_${p.id}" class="text-muted" style="font-size:11px;margin-top:3px;"></div>
      </div>
      <div class="btn-group" style="gap:6px;">
        <button class="btn btn-small btn-secondary" onclick="personaTestIp('${p.id}')">\u6D4B\u51FA\u53E3IP</button>
        <button class="btn btn-small btn-secondary" style="color:#e55;" onclick="personaDelete('${p.id}','${escapeHtml(p.email)}')">\u5220\u9664</button>
      </div>
    </div>`).join("");
  }
  window.personaTestIp = async (id) => {
    const el = document.getElementById("ip_" + id);
    if (el) el.innerHTML = "\u6D4B\u8BD5\u4E2D\u2026\uFF08\u4F1A\u5F00\u4E00\u4E0B\u8BE5\u8EAB\u4EFD\u7684\u6D4F\u89C8\u5668\uFF09";
    try {
      const r = await invoke2("persona_test_ip", { id });
      if (el) el.innerHTML = "\u{1F30D} " + escapeHtml(r);
    } catch (e) {
      if (el) el.innerHTML = `<span style="color:#e55;">\u6D4B\u8BD5\u5931\u8D25\uFF1A${escapeHtml("" + e)}</span>`;
    }
  };
  window.personaDelete = async (id, email) => {
    if (!confirm(`\u5220\u9664\u8EAB\u4EFD ${email}\uFF1F
\u4F1A\u540C\u65F6\u5220\u6389\u5B83\u7684\u72EC\u7ACB\u6D4F\u89C8\u5668\u5E76\u91CA\u653E\u51FA\u53E3\u8282\u70B9\u3002`)) return;
    try {
      await invoke2("persona_delete", { id });
      showToast("\u5DF2\u5220\u9664", "success");
      refreshPersonas();
      refreshAirport();
    } catch (e) {
      showToast("\u5220\u9664\u5931\u8D25\uFF1A" + e, "error");
    }
  };
})();
