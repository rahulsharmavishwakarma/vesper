PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE sessions (
                 id TEXT PRIMARY KEY NOT NULL,
                 created_at TEXT NOT NULL,
                 review_status TEXT NOT NULL CHECK(review_status IN ('pending', 'approved')),
                 record_json TEXT NOT NULL
             );
INSERT INTO sessions VALUES('0c226b51-1bce-4de0-8a3a-f38d388c4e20','2026-10-29T12:03:57.639Z','approved','{"id":"0c226b51-1bce-4de0-8a3a-f38d388c4e20","createdAt":"2026-10-29T12:03:57.639Z","originalFilename":"synthetic.wav","audioRelativePath":"audio/0c226b51-1bce-4de0-8a3a-f38d388c4e20.wav","audioSha256":"5184b2b32fcb6b2656087fcf5e9fb63d8548c6dea9ef60c8974faba124ba8075","audioBytes":3244,"rawTranscript":"send the atlas notes tomorrow","speakerTimeline":null,"cleanedTranscript":"Send the Atlas notes tomorrow.","personalizedTranscript":null,"draftTranscript":null,"draftRevision":0,"personalizationEvidence":{"profileRevision":0,"applied":[],"ambiguousMatches":0,"suggestedIntent":null,"correctionsEnabled":false,"budgetExceeded":false,"gate":[],"hintHash":"","gateShadow":[],"formatting":[]},"personalizationProject":"atlas","projectSelection":{"project":"atlas","source":"manual","ruleId":null},"personalizationRejected":false,"correctedTranscript":"Send the Atlas notes tomorrow.","verifiedTranscript":null,"reviewHistory":[{"id":"ace4d5b7-2b68-4471-a38b-8dedc6993533","targetKind":"cleanup_correction","text":"Send the Atlas notes tomorrow.","timestamp":"2026-10-29T12:03:57.641Z"}],"reviewStatus":"approved","transcriptFeedback":null,"configSnapshot":{"schemaVersion":1,"models":{"asr":"CohereLabs/cohere-transcribe-03-2026","asrEngine":"automatic","rewriter":"Qwen/Qwen3.5-0.8B","customAsr":null,"customRewriter":null},"cleanupEnabled":true,"audioFrontend":{"vadEnabled":false,"autoLevelEnabled":false,"noiseReduction":"off"},"language":"en","cleanupPreset":"minimal","customCleanupInstruction":"","rewriteQuantization":"automatic","computeBackend":"auto","maxRecordingSeconds":0,"globalShortcut":"CommandOrControl+Shift+Space","recordingMode":"toggle","holdShortcut":null,"toggleShortcut":null,"dictationOutput":"app","floatingBarEnabled":true,"captureDesign":"split","notificationsEnabled":false,"collectTrainingData":true,"improvementMode":"review_required"},"modelVersions":{"asr":"synthetic"},"pipelineEvents":[]}');
CREATE TABLE personalization (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 profile_json TEXT NOT NULL
             );
INSERT INTO personalization VALUES(1,'{"schemaVersion":4,"revision":4,"speechBiasEnabled":false,"outputIntent":"verbatim","automaticWriting":false,"preserveFillers":false,"applyCorrections":true,"useRememberedIntent":false,"activeProject":"atlas","projectMode":"manual","projectRules":[],"automaticWorkspaces":false,"learnFromAppEdits":false,"appFieldWords":false,"formatDictation":true,"automaticLearning":true,"learningEpoch":1,"vocabulary":[{"id":"fixture-word","phrase":"Vesper","language":"auto","context":"","enabled":true,"aliases":[],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T12:03:57.643Z","automaticSource":null,"confirmed":true,"usageCount":0},{"id":"9c6674eb-9717-4bd5-a3b6-896b797fe328","phrase":"kubernetes","language":"en","context":"cluster deploy","enabled":true,"aliases":["kubernets"],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T12:03:57.666Z","automaticSource":"3a0d0976-21b0-4589-9d28-6546c9db2865","confirmed":false,"usageCount":0}],"examples":[{"id":"11188763-e6dc-49e8-95b5-fbcbd2c00064","sessionId":"0c226b51-1bce-4de0-8a3a-f38d388c4e20","reviewRevisionId":"ace4d5b7-2b68-4471-a38b-8dedc6993533","input":"send the atlas notes tomorrow","output":"Send the Atlas notes tomorrow.","language":"en","preset":"minimal","intent":"note","enabled":true,"project":"atlas"}]}');
CREATE TABLE automatic_learning (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 state_json TEXT NOT NULL
             );
INSERT INTO automatic_learning VALUES(1,'{"schemaVersion":2,"policyVersion":6,"candidates":[{"id":"3a0d0976-21b0-4589-9d28-6546c9db2865","phrase":"kubernetes","alias":"kubernets","language":"en","project":"","context":"cluster deploy","state":"active","support":[{"sourceKey":"audio-one","episodeId":"one","eventId":"one:0","teachingTerms":["cluster","deploy"],"recognizer":""},{"sourceKey":"audio-two","episodeId":"two","eventId":"two:0","teachingTerms":["cluster","deploy"],"recognizer":""}],"observedCount":0,"lastSeenMs":1793275437666,"reason":"Learned from ordinary user edits; not an audio review."}],"events":[{"id":"one:0","context":{"resultId":"one","sourceKey":"audio-one","episodeId":"one","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793275437665},{"id":"two:0","context":{"resultId":"two","sourceKey":"audio-two","episodeId":"two","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793275437666}],"expiredCandidates":0,"reverts":[],"breakerTrippedMs":null}');
CREATE TABLE policy_shadow (id INTEGER PRIMARY KEY CHECK(id=1), state_json TEXT NOT NULL);
CREATE TABLE policy_shadow_job (id INTEGER PRIMARY KEY CHECK(id=1), request_json TEXT NOT NULL);
CREATE TABLE workspace_store (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 salt TEXT NOT NULL,
                 state_json TEXT NOT NULL
             );
CREATE TABLE workspace_detection (
                 application TEXT PRIMARY KEY NOT NULL,
                 dictations INTEGER NOT NULL,
                 file INTEGER NOT NULL,
                 folder INTEGER NOT NULL,
                 web INTEGER NOT NULL,
                 none INTEGER NOT NULL,
                 under_1ms INTEGER NOT NULL,
                 under_5ms INTEGER NOT NULL,
                 under_30ms INTEGER NOT NULL,
                 slower INTEGER NOT NULL
             );
CREATE TABLE semantic_shadow (
                 result_id TEXT PRIMARY KEY NOT NULL,
                 created_at INTEGER NOT NULL,
                 request_json TEXT NOT NULL,
                 report_json TEXT NOT NULL
             );
CREATE TABLE term_inventory (
    id INTEGER PRIMARY KEY,
    surface TEXT NOT NULL UNIQUE,
    key TEXT NOT NULL,
    kind TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('asserted', 'authored')),
    status TEXT NOT NULL CHECK(status IN ('unconfirmed', 'active', 'suspended', 'removed')),
    spoken_json TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    doc_freq INTEGER NOT NULL,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL
);
CREATE TABLE term_mining_state (key TEXT PRIMARY KEY NOT NULL, value_json TEXT NOT NULL);
CREATE TABLE term_mining_runs (id INTEGER PRIMARY KEY, started_ms INTEGER NOT NULL, run_json TEXT NOT NULL);
CREATE TABLE app_learning_coverage (
        application TEXT PRIMARY KEY NOT NULL,
        readbacks INTEGER NOT NULL DEFAULT 0,
        unchanged INTEGER NOT NULL DEFAULT 0,
        edited INTEGER NOT NULL DEFAULT 0,
        unaligned INTEGER NOT NULL DEFAULT 0,
        unreadable INTEGER NOT NULL DEFAULT 0,
        cleared INTEGER NOT NULL DEFAULT 0,
        field_reads INTEGER NOT NULL DEFAULT 0,
        field_unreadable INTEGER NOT NULL DEFAULT 0,
        field_terms INTEGER NOT NULL DEFAULT 0
    );
CREATE TABLE paste_coverage (
        application TEXT PRIMARY KEY NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0,
        sent INTEGER NOT NULL DEFAULT 0,
        no_field INTEGER NOT NULL DEFAULT 0,
        no_permission INTEGER NOT NULL DEFAULT 0,
        secure INTEGER NOT NULL DEFAULT 0,
        field_changed INTEGER NOT NULL DEFAULT 0,
        clipboard INTEGER NOT NULL DEFAULT 0,
        other INTEGER NOT NULL DEFAULT 0,
        last_failure TEXT NOT NULL DEFAULT ''
    );
CREATE INDEX term_inventory_key ON term_inventory(key);
CREATE INDEX term_inventory_status ON term_inventory(status);
COMMIT;
