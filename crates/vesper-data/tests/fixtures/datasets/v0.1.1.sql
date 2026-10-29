PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE sessions (
                 id TEXT PRIMARY KEY NOT NULL,
                 created_at TEXT NOT NULL,
                 review_status TEXT NOT NULL CHECK(review_status IN ('pending', 'approved')),
                 record_json TEXT NOT NULL
             );
INSERT INTO sessions VALUES('aaca438e-2f9f-4884-b498-7e7a82de562b','2026-10-29T06:45:10.543Z','approved','{"id":"aaca438e-2f9f-4884-b498-7e7a82de562b","createdAt":"2026-10-29T06:45:10.543Z","originalFilename":"synthetic.wav","audioRelativePath":"audio/aaca438e-2f9f-4884-b498-7e7a82de562b.wav","audioSha256":"5184b2b32fcb6b2656087fcf5e9fb63d8548c6dea9ef60c8974faba124ba8075","audioBytes":3244,"rawTranscript":"send the atlas notes tomorrow","speakerTimeline":null,"cleanedTranscript":"Send the Atlas notes tomorrow.","personalizedTranscript":null,"draftTranscript":null,"draftRevision":0,"personalizationEvidence":{"profileRevision":0,"applied":[],"ambiguousMatches":0,"suggestedIntent":null,"correctionsEnabled":false,"budgetExceeded":false},"personalizationProject":"atlas","projectSelection":{"project":"atlas","source":"manual","ruleId":null},"personalizationRejected":false,"correctedTranscript":"Send the Atlas notes tomorrow.","verifiedTranscript":null,"reviewHistory":[{"id":"86b5b49a-4908-4b67-b974-072c5f7c56fc","targetKind":"cleanup_correction","text":"Send the Atlas notes tomorrow.","timestamp":"2026-10-29T06:45:10.549Z"}],"reviewStatus":"approved","transcriptFeedback":null,"configSnapshot":{"schemaVersion":1,"models":{"asr":"CohereLabs/cohere-transcribe-03-2026","asrEngine":"automatic","rewriter":"Qwen/Qwen3.5-0.8B","customAsr":null,"customRewriter":null},"cleanupEnabled":true,"audioFrontend":{"vadEnabled":false,"autoLevelEnabled":false,"noiseReduction":"off"},"language":"en","cleanupPreset":"minimal","customCleanupInstruction":"","rewriteQuantization":"automatic","computeBackend":"auto","maxRecordingSeconds":0,"globalShortcut":"CommandOrControl+Shift+Space","recordingMode":"toggle","holdShortcut":null,"toggleShortcut":null,"dictationOutput":"app","floatingBarEnabled":true,"captureDesign":"split","notificationsEnabled":false,"collectTrainingData":true,"improvementMode":"review_required"},"modelVersions":{"asr":"synthetic"},"pipelineEvents":[]}');
CREATE TABLE personalization (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 profile_json TEXT NOT NULL
             );
INSERT INTO personalization VALUES(1,'{"schemaVersion":4,"revision":4,"speechBiasEnabled":false,"outputIntent":"verbatim","automaticWriting":false,"preserveFillers":false,"applyCorrections":true,"useRememberedIntent":false,"activeProject":"atlas","projectMode":"manual","projectRules":[],"automaticLearning":true,"learningEpoch":1,"vocabulary":[{"id":"fixture-word","phrase":"Vesper","language":"auto","context":"","enabled":true,"aliases":[],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T06:45:10.549Z","automaticSource":null,"confirmed":true,"usageCount":0},{"id":"0b13a406-229d-41d6-8b2d-f0793e400a3f","phrase":"kubernetes","language":"en","context":"cluster deploy","enabled":true,"aliases":["kubernets"],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T06:45:10.550Z","automaticSource":"87e7b8a9-8bd9-42a3-af96-49b69db80668","confirmed":false,"usageCount":0}],"examples":[{"id":"6e93c586-26a9-4a25-9d25-3708a585375d","sessionId":"aaca438e-2f9f-4884-b498-7e7a82de562b","reviewRevisionId":"86b5b49a-4908-4b67-b974-072c5f7c56fc","input":"send the atlas notes tomorrow","output":"Send the Atlas notes tomorrow.","language":"en","preset":"minimal","intent":"note","enabled":true,"project":"atlas"}]}');
CREATE TABLE automatic_learning (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 state_json TEXT NOT NULL
             );
INSERT INTO automatic_learning VALUES(1,'{"schemaVersion":2,"policyVersion":4,"candidates":[{"id":"87e7b8a9-8bd9-42a3-af96-49b69db80668","phrase":"kubernetes","alias":"kubernets","language":"en","project":"","context":"cluster deploy","state":"active","support":[{"sourceKey":"audio-one","episodeId":"one","eventId":"one:0","teachingTerms":["cluster","deploy"]},{"sourceKey":"audio-two","episodeId":"two","eventId":"two:0","teachingTerms":["cluster","deploy"]}],"observedCount":0,"lastSeenMs":1793256310550,"reason":"Learned from ordinary user edits; not an audio review."}],"events":[{"id":"one:0","context":{"resultId":"one","sourceKey":"audio-one","episodeId":"one","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793256310550},{"id":"two:0","context":{"resultId":"two","sourceKey":"audio-two","episodeId":"two","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793256310550}],"expiredCandidates":0}');
CREATE TABLE policy_shadow (id INTEGER PRIMARY KEY CHECK(id=1), state_json TEXT NOT NULL);
CREATE TABLE policy_shadow_job (id INTEGER PRIMARY KEY CHECK(id=1), request_json TEXT NOT NULL);
CREATE TABLE semantic_shadow (
                 result_id TEXT PRIMARY KEY NOT NULL,
                 created_at INTEGER NOT NULL,
                 request_json TEXT NOT NULL,
                 report_json TEXT NOT NULL
             );
COMMIT;
