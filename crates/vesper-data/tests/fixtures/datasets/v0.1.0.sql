PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE sessions (
                 id TEXT PRIMARY KEY NOT NULL,
                 created_at TEXT NOT NULL,
                 review_status TEXT NOT NULL CHECK(review_status IN ('pending', 'approved')),
                 record_json TEXT NOT NULL
             );
INSERT INTO sessions VALUES('e59e4894-413e-4ede-9bbc-9dc27df93556','2026-10-29T06:45:34.591Z','approved','{"id":"e59e4894-413e-4ede-9bbc-9dc27df93556","createdAt":"2026-10-29T06:45:34.591Z","originalFilename":"synthetic.wav","audioRelativePath":"audio/e59e4894-413e-4ede-9bbc-9dc27df93556.wav","audioSha256":"5184b2b32fcb6b2656087fcf5e9fb63d8548c6dea9ef60c8974faba124ba8075","audioBytes":3244,"rawTranscript":"send the atlas notes tomorrow","speakerTimeline":null,"cleanedTranscript":"Send the Atlas notes tomorrow.","personalizedTranscript":null,"draftTranscript":null,"draftRevision":0,"personalizationEvidence":{"profileRevision":0,"applied":[],"ambiguousMatches":0,"suggestedIntent":null,"correctionsEnabled":false,"budgetExceeded":false},"personalizationProject":"atlas","projectSelection":{"project":"atlas","source":"manual","ruleId":null},"personalizationRejected":false,"correctedTranscript":"Send the Atlas notes tomorrow.","verifiedTranscript":null,"reviewHistory":[{"id":"e482338f-6f9b-4d4d-b3ea-3882b5e0c451","targetKind":"cleanup_correction","text":"Send the Atlas notes tomorrow.","timestamp":"2026-10-29T06:45:34.596Z"}],"reviewStatus":"approved","transcriptFeedback":null,"configSnapshot":{"schemaVersion":1,"models":{"asr":"CohereLabs/cohere-transcribe-03-2026","rewriter":"Qwen/Qwen3.5-0.8B","customAsr":null,"customRewriter":null},"cleanupEnabled":true,"audioFrontend":{"vadEnabled":false,"autoLevelEnabled":false,"noiseReduction":"off"},"language":"en","cleanupPreset":"minimal","customCleanupInstruction":"","rewriteQuantization":"q4_0","computeBackend":"auto","maxRecordingSeconds":0,"globalShortcut":"CommandOrControl+Shift+Space","recordingMode":"toggle","holdShortcut":null,"toggleShortcut":null,"dictationOutput":"app","floatingBarEnabled":true,"captureDesign":"split","notificationsEnabled":false,"collectTrainingData":true,"improvementMode":"review_required"},"modelVersions":{"asr":"synthetic"},"pipelineEvents":[]}');
CREATE TABLE personalization (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 profile_json TEXT NOT NULL
             );
INSERT INTO personalization VALUES(1,'{"schemaVersion":4,"revision":4,"speechBiasEnabled":false,"outputIntent":"verbatim","automaticWriting":false,"preserveFillers":false,"applyCorrections":true,"useRememberedIntent":false,"activeProject":"atlas","projectMode":"manual","projectRules":[],"automaticLearning":true,"learningEpoch":1,"vocabulary":[{"id":"fixture-word","phrase":"Vesper","language":"auto","context":"","enabled":true,"aliases":[],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T06:45:34.597Z","automaticSource":null,"confirmed":true,"usageCount":0},{"id":"2bf7a346-0b49-4704-bc7c-319beadaff1e","phrase":"kubernetes","language":"en","context":"cluster deploy","enabled":true,"aliases":["kubernets"],"project":"","pronunciation":false,"sources":[],"learnedAt":"2026-10-29T06:45:34.597Z","automaticSource":"fec833e7-4ca3-4b37-8edb-de6364132e96","confirmed":false,"usageCount":0}],"examples":[{"id":"81af6a06-f9fc-456f-9268-08d84acf2d68","sessionId":"e59e4894-413e-4ede-9bbc-9dc27df93556","reviewRevisionId":"e482338f-6f9b-4d4d-b3ea-3882b5e0c451","input":"send the atlas notes tomorrow","output":"Send the Atlas notes tomorrow.","language":"en","preset":"minimal","intent":"note","enabled":true,"project":"atlas"}]}');
CREATE TABLE automatic_learning (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 state_json TEXT NOT NULL
             );
INSERT INTO automatic_learning VALUES(1,'{"schemaVersion":2,"policyVersion":4,"candidates":[{"id":"fec833e7-4ca3-4b37-8edb-de6364132e96","phrase":"kubernetes","alias":"kubernets","language":"en","project":"","context":"cluster deploy","state":"active","support":[{"sourceKey":"audio-one","episodeId":"one","eventId":"one:0","teachingTerms":["cluster","deploy"]},{"sourceKey":"audio-two","episodeId":"two","eventId":"two:0","teachingTerms":["cluster","deploy"]}],"observedCount":0,"lastSeenMs":1793256334597,"reason":"Learned from ordinary user edits; not an audio review."}],"events":[{"id":"one:0","context":{"resultId":"one","sourceKey":"audio-one","episodeId":"one","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793256334597},{"id":"two:0","context":{"resultId":"two","sourceKey":"audio-two","episodeId":"two","language":"en","project":"","modelId":"","learningEpoch":1,"learningEnabled":true},"kind":"user_edit","pair":{"heard":"kubernets","intended":"kubernetes"},"retry":null,"timestampMs":1793256334597}],"expiredCandidates":0}');
CREATE TABLE policy_shadow (id INTEGER PRIMARY KEY CHECK(id=1), state_json TEXT NOT NULL);
CREATE TABLE policy_shadow_job (id INTEGER PRIMARY KEY CHECK(id=1), request_json TEXT NOT NULL);
CREATE TABLE semantic_shadow (
                 result_id TEXT PRIMARY KEY NOT NULL,
                 created_at INTEGER NOT NULL,
                 request_json TEXT NOT NULL,
                 report_json TEXT NOT NULL
             );
COMMIT;
