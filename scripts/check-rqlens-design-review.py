#!/usr/bin/env python3
"""Real CLI/policy/SARIF tests for source-linked declared human design reviews.

EVERY qualifying human decision generated here is a SYNTHETIC TEST FIXTURE ONLY,
not a real person's approval of Scratchpad. Actual project drafts stay AI-authored
and pending-human. Run inside nix develop with frozen frontend/helper sources.
"""

import argparse
import copy
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


spec = importlib.util.spec_from_file_location(
    "rqlens_profiles", Path(__file__).with_name("check-rqlens-profiles.py")
)
profiles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profiles)
RULE = "rust.design.required-review"
UNIT = "unmet-required-human-review-obligation"
TOPIC = "construction-builders"
IDENTITY = "SYNTHETIC TEST FIXTURE ONLY — not a real human or project approval"
RATIONALE = (
    "SYNTHETIC TEST FIXTURE ONLY. This declared human identity is not authenticated "
    "and represents no real person. The toy source was considered solely to test "
    "record binding, policy counts and validity handling, not to approve Scratchpad "
    "or establish design adequacy, safety or guide compliance."
)
SOURCE = '''//! SYNTHETIC review fixture, not project approval.

/// A toy construction API for review-record tests.
pub struct Builder {
    /// A retained value.
    pub value: u8,
}

impl Builder {
    /// Construct the toy value.
    #[must_use]
    pub fn new(value: u8) -> Self {
        Self { value }
    }
}
'''


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def stamp(value):
    return value.isoformat(timespec="seconds").replace("+00:00", "Z")


def config_text(directory, helper, topic=TOPIC, sources=("src",), max_age=90,
                enabled=True, enforced=True, required=True):
    text = (
        'project_root="."\nsource_roots=["src"]\noutput_dir="artifacts"\n'
        '[rust]\nidentity_resolution="auto"\nidentity_offline=true\n'
        f'helper_manifest={json.dumps(str(helper))}\n'
        '[verification]\nworkspace=false\nall_targets=false\nlocked=true\ntimeout_seconds=60\n'
    )
    if enabled:
        text += (
            '[design_review]\nmanifest="design-reviews.json"\n'
            '[[design_review.requirements]]\nid="synthetic-construction-v1"\n'
            f'topic={json.dumps(topic)}\nsources={json.dumps(list(sources))}\n'
            f'required={str(required).lower()}\nmax_age_days={max_age}\n'
        )
    if enforced:
        text += f'[policy.rules."{RULE}"]\nlevel="error"\nmax=0\n'
    return text


def setup(root, name, helper):
    directory = root / name
    profiles.setup(directory, SOURCE, None, helper, shutil.which("cargo"))
    (directory / "src/part.rs").write_text("//! Another declared source member.\npub fn helper() {}\n")
    config = directory / "rqlens.toml"
    config.write_text(config_text(directory, helper))
    return directory, config


def cli(tool, directory, action, config=None, name=None, environment=None):
    args = [tool, "design-review", action]
    if config:
        args += ["--config", config]
    name = name or action
    code = profiles.execute(args, directory, name, environment)
    require(code == 0, f"{action} exited {code}; inspect {directory}/{name} logs")
    return json.loads((directory / (name + ".stdout.log")).read_text())


def store(directory, manifest):
    (directory / "design-reviews.json").write_text(json.dumps(manifest, indent=2) + "\n")


def accepted(template, decision="accepted", expires=None):
    manifest = copy.deepcopy(template)
    now = datetime.now(timezone.utc) - timedelta(seconds=1)
    for record in manifest["records"]:
        record.update({"decision": decision, "reviewer": {"kind": "human", "identity": IDENTITY},
                       "rationale": RATIONALE, "drafted_at": stamp(now - timedelta(seconds=1)),
                       "reviewed_at": stamp(now), "expires_at": stamp(expires or now + timedelta(days=1))})
    return manifest


def evaluate(tool, directory, config, name, expected, states=(), refresh_stale=False):
    view = cli(tool, directory, "inspect", config, name + "-inspect")
    require(view["evidence"]["identity_authenticated"] is False, "declared identity falsely authenticated")
    observed_states = [row["state"] for row in view["evidence"].get("obligations", [])]
    require(not states or all(state in observed_states for state in states),
            f"wrong live obligation states: {observed_states}; expected {states}")
    code = profiles.execute([tool, "check", "--config", config], directory, name + "-check")
    report = json.loads((directory / "artifacts/policy_report.json").read_text())
    evaluation = next(row for row in report["policy_rule_evaluations"] if row["rule_id"] == RULE)
    require(report["passed"] == (expected == 0) and (code == 0) == (expected == 0),
            f"incorrect policy/command pass: {evaluation}")
    require(evaluation["counting_unit"] == UNIT, "review counting unit drift")
    if expected is None:
        require(evaluation["status"] == "incomplete" and evaluation["observed"] is None and
                evaluation["evaluation_complete"] is False, "unknown review coverage became zero")
    else:
        require(evaluation["evaluation_complete"] is True and evaluation["observed"] == expected,
                f"wrong exact obligation count: {evaluation}")
    sarif = directory / (name + ".sarif")
    if refresh_stale:
        require(report["artifact_status"]["api_health.json"]["status"] == "stale",
                "expected separate cached artifact freshness invalidation")
        require(profiles.execute([tool, "sarif", "--config", config, "--output", sarif],
                                 directory, name + "-stale-sarif") != 0,
                "stale generic evidence unexpectedly exported")
        require(profiles.execute([tool, "measure", "api-health", "--config", config],
                                 directory, name + "-refresh-api") == 0,
                "refresh of intentionally stale generic evidence failed")
    require(profiles.execute([tool, "sarif", "--config", config, "--output", sarif],
                             directory, name + "-sarif") == 0, "SARIF generation failed")
    data = json.loads(sarif.read_text())
    findings = [row["properties"]["rqlens_finding"] for row in data["runs"][0]["results"]
                if row["ruleId"] == RULE]
    require(sorted(row["id"] for row in view["findings"]) == sorted(row["id"] for row in findings),
            "policy/inspect/SARIF stable obligation identities disagree")
    if expected is not None:
        require(len(findings) == expected and len({row["requirement_id"] for row in findings}) == expected,
                "SARIF double-counted review issues instead of obligations")
    for finding in findings:
        require(finding["counting_unit"] == UNIT and finding["evidence_kind"] == "declared-human-review-not-syntax-or-compiler-proof",
                "human declarations misrepresented as source/compiler proof")
    (directory / (name + "-policy.json")).write_text(json.dumps(report, indent=2) + "\n")
    return {"evaluation": evaluation, "states": observed_states, "findings": findings,
            "source_bindings": [row["binding"] for row in view["evidence"].get("obligations", [])],
            "artifact_status": report["artifact_status"].get("api_health.json")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rqlens", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--helper-manifest", type=Path, required=True)
    args = parser.parse_args()
    source = args.rqlens.resolve(strict=True);helper = args.helper_manifest.resolve(strict=True)
    root = args.output.resolve();root.mkdir(parents=True, exist_ok=False)
    tool = root / source.name;shutil.copy2(source, tool)
    results = {"passed": False, "tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
               "synthetic_acceptances_only": True, "cases": {}}

    def case(name, action):
        try:
            results["cases"][name] = {"passed": True, "evidence": action()}
        except (OSError, ValueError, KeyError, StopIteration, AssertionError, RuntimeError, subprocess.TimeoutExpired) as error:
            results["cases"][name] = {"passed": False, "error": str(error)}
        result = results["cases"][name]
        print(name, "PASS" if result["passed"] else "FAIL", result.get("error", ""), flush=True)

    def fixture(name):
        directory,config = setup(root, name, helper)
        template = cli(tool, directory, "prepare", config)
        require(template["schema_version"] == 1 and all(record["decision"] == "pending-human" and
                record["reviewer"]["kind"] == "ai" and record["reviewed_at"] is None
                for record in template["records"]), "prepare forged a human approval")
        return directory,config,template

    try:
        results["schema"] = cli(tool, root, "schema")
        results["rubric"] = cli(tool, root, "rubric")

        def human_fixture():
            directory,config,template = fixture("synthetic-acceptance")
            record = template["records"][0]
            require(record["binding"]["algorithm"] == "fnv1a64-design-source-v1" and
                    [row["path"] for row in record["binding"]["files"]] == ["src/lib.rs", "src/part.rs"],
                    "source binding unit/set is not explicit")
            store(directory, accepted(template))
            require(profiles.execute([tool, "measure", "api-health", "--config", config],
                                     directory, "measure") == 0, "design-review producer failed")
            return evaluate(tool, directory, config, "accepted", 0, ("accepted",))
        case("synthetic-human-acceptance", human_fixture)

        for topic in results["rubric"]["topics"]:
            if topic == TOPIC: continue
            def declared_topic(topic=topic):
                directory,config=setup(root,"synthetic-topic-"+topic,helper)
                config.write_text(config_text(directory,helper,topic=topic))
                template=cli(tool,directory,"prepare",config);store(directory,accepted(template))
                return evaluate(tool,directory,config,"observed",0,("accepted",))
            case("synthetic-topic-"+topic,declared_topic)

        for name,state,mutator,expected in (
            ("pending-ai-draft", "pending-human", lambda m: m, 1),
            ("missing-record", "missing-record", lambda m: {**m, "records": []}, 1),
            ("changes-requested", "changes-requested", lambda m: mutate(m, decision="changes-requested"), 1),
            ("accepted-ai-is-not-human", "non-human-or-unidentified-decision", lambda m: mutate(accepted(m), reviewer={"kind":"ai","identity":"AI TEST FIXTURE"}), 1),
            ("accepted-synthetic-kind-is-not-human", "non-human-or-unidentified-decision", lambda m: mutate(accepted(m), reviewer={"kind":"synthetic","identity":IDENTITY}), 1),
            ("wrong-topic", "stale-requirement", lambda m: mutate(accepted(m), topic="operator-semantics"), 1),
            ("wrong-source-binding", "stale-source", lambda m: wrong_binding(accepted(m)), 1),
            ("unknown-binding", "unsupported-binding-algorithm", lambda m: wrong_binding(accepted(m), algorithm=True), None),
            ("future-review", "future-review-time", lambda m: future(accepted(m)), 1),
            ("short-rationale", "insufficient-rationale", lambda m: mutate(accepted(m), rationale="SYNTHETIC ONLY"), 1),
            ("malformed-review-time", "missing-or-invalid-review-time", lambda m: mutate(accepted(m), reviewed_at="not-a-time"), 1),
            ("unbounded-review-validity", "invalid-validity-window", lambda m: mutate(accepted(m), expires_at=stamp(datetime.now(timezone.utc)+timedelta(days=366))), 1),
            ("synthetic-not-applicable", "not-applicable", lambda m: accepted(m,decision="not-applicable"), 0),
            ("duplicate-records", "invalid-review-contract", lambda m: {**accepted(m), "records": accepted(m)["records"] * 2}, None),
            ("future-schema", "invalid-review-contract", lambda m: {**m,"schema_version":999}, None),
            ("unknown-requirement-binding", "invalid-review-contract", lambda m: mutate(accepted(m), requirement_id="undeclared-test-id"), None),
        ):
            def observed(name=name,state=state,mutator=mutator,expected=expected):
                directory,config,template=fixture(name);store(directory,mutator(template))
                return evaluate(tool,directory,config,"observed",expected,(state,))
            case(name,observed)

        def missing_manifest():
            directory,config=setup(root,"missing-manifest",helper)
            return evaluate(tool,directory,config,"observed",1,("missing-record",))
        case("missing-manifest-keeps-config-obligation",missing_manifest)

        def read_only():
            directory,config=setup(root,"read-only-no-tool-execution",helper)
            shim=directory/"shim";shim.mkdir();marker=directory/"tool-was-executed"
            for command in ["cargo","rustc","rustdoc"]:
                wrapper=shim/command
                wrapper.write_text('#!/bin/sh\nprintf tool >> '+str(marker)+'\nexit 91\n')
                wrapper.chmod(0o755)
            env=os.environ.copy();env["PATH"]=str(shim)+os.pathsep+env["PATH"]
            prepared=cli(tool,directory,"prepare",config,environment=env)
            cli(tool,directory,"fingerprint",config,environment=env)
            require(not marker.exists(),"read-only design inspection executed Cargo/compiler code")
            require(all(record["decision"]=="pending-human" for record in prepared["records"]),"prepare forged acceptance")
            return {"tools_executed":False,"decisions":[r["decision"] for r in prepared["records"]]}
        case("read-only-review-cli-avoids-build-tools",read_only)

        def source_changes():
            directory,config,template=fixture("source-transitions");manifest=accepted(template);store(directory,manifest)
            result={"before":evaluate(tool,directory,config,"before",0)}
            path=directory/"src/lib.rs";original=path.read_text();path.write_text(original+"\n// Changed reviewed source.\n")
            result["edit"]=evaluate(tool,directory,config,"edit",1,("stale-source",));path.write_text(original)
            added=directory/"src/added.rs";added.write_text("pub fn added() {}\n")
            result["add"]=evaluate(tool,directory,config,"add",1,("stale-source",));added.unlink()
            part=directory/"src/part.rs";content=part.read_text();part.unlink()
            result["remove"]=evaluate(tool,directory,config,"remove",1,("stale-source",));part.write_text(content)
            result["exact-restoration"]=evaluate(tool,directory,config,"restored",0)
            return result
        case("source-edit-add-remove-binding",source_changes)

        def wrong_scope():
            directory,config,template=fixture("wrong-declared-scope");store(directory,accepted(template))
            shutil.copytree(directory/"src",directory/"other")
            config.write_text(config_text(directory,helper,sources=("other",)))
            current=cli(tool,directory,"prepare",config,"new-scope")
            require(current["records"][0]["binding"]["digest"]!=template["records"][0]["binding"]["digest"],
                    "source binding ignores project-relative paths")
            return evaluate(tool,directory,config,"observed",1,("stale-requirement",))
        case("identical-bytes-wrong-declared-scope",wrong_scope)

        def metadata_changes():
            directory,config,template=fixture("metadata-transitions");store(directory,accepted(template))
            require(profiles.execute([tool,"measure","api-health","--config",config],directory,"measure") == 0,"producer failed")
            binding=template["records"][0]["binding"]
            manifest=accepted(template);manifest["records"][0]["rationale"] += " Additional SYNTHETIC TEST metadata only."
            store(directory,manifest)
            current=cli(tool,directory,"prepare",config,"prepare-after-metadata")
            require(current["records"][0]["binding"]==binding,"review manifest became part of its own source digest")
            metadata=evaluate(tool,directory,config,"metadata",0,refresh_stale=True)
            state=metadata["artifact_status"]
            require(state and state["status"].lower()=="stale","review metadata edit did not invalidate cached artifact freshness")
            config.write_text(config_text(directory,helper,max_age=30))
            requirement=evaluate(tool,directory,config,"requirement",1,("stale-requirement",),refresh_stale=True)
            current=cli(tool,directory,"prepare",config,"prepare-after-requirement")
            require(current["records"][0]["binding"]==binding and current["records"][0]["requirement_digest"]!=template["records"][0]["requirement_digest"],
                    "requirement metadata improperly coupled to source binding")
            return {"manifest-metadata":metadata,"requirement-metadata":requirement}
        case("metadata-freshness-not-self-reference",metadata_changes)

        def expiry():
            directory,config,template=fixture("expiry-live")
            deadline=datetime.now(timezone.utc)+timedelta(seconds=45);store(directory,accepted(template,expires=deadline))
            manifest_hash=hashlib.sha256((directory/"design-reviews.json").read_bytes()).hexdigest()
            result={"before":evaluate(tool,directory,config,"before",0)}
            while datetime.now(timezone.utc)<=deadline: time.sleep(0.25)
            result["after"]=evaluate(tool,directory,config,"after",1,("expired",))
            require(result["before"]["source_bindings"]==result["after"]["source_bindings"] and
                    hashlib.sha256((directory/"design-reviews.json").read_bytes()).hexdigest()==manifest_hash,
                    "expiry test changed source/review metadata rather than only time")
            return result
        case("expiry-rechecked-without-source-edit",expiry)

        def malformed():
            directory,config=setup(root,"malformed",helper);(directory/"design-reviews.json").write_text("not JSON")
            return evaluate(tool,directory,config,"observed",None,("invalid-review-contract",))
        case("malformed-manifest",malformed)

        def disabled_required():
            directory,config=setup(root,"disabled-required",helper)
            config.write_text(config_text(directory,helper,enabled=False))
            return evaluate(tool,directory,config,"observed",None)
        case("disabled-required-rule-not-clean",disabled_required)

        def optional_mode():
            directory,config=setup(root,"disabled-legacy",helper)
            config.write_text(config_text(directory,helper,enabled=False,enforced=False))
            view=cli(tool,directory,"inspect",config)
            require(view["evidence"]["status"]=="disabled" and view["evidence"]["accepted_required"] is None,"disabled mode claims human acceptance")
            require(profiles.execute([tool,"check","--config",config],directory,"check") == 0,"legacy rule-free gate changed")
            return view["evidence"]
        case("disabled-preserves-legacy-gate",optional_mode)

        def empty_required():
            directory,config,template=fixture("empty-required-scope")
            config.write_text(config_text(directory,helper,required=False));store(directory,template)
            return evaluate(tool,directory,config,"observed",None)
        case("enforced-empty-required-scope-not-zero",empty_required)

        for name,sources in (("self-referential-scope",("design-reviews.json",)),("unsupported-source",("missing-source",))):
            def scope(name=name,sources=sources):
                directory,config,template=fixture(name);store(directory,accepted(template))
                config.write_text(config_text(directory,helper,sources=sources))
                return evaluate(tool,directory,config,"observed",None,("unsupported-source-scope",))
            case(name,scope)
        results["passed"]=all(result["passed"] for result in results["cases"].values())
    finally:
        (root/"summary.json").write_text(json.dumps(results,indent=2)+"\n")
    return 0 if results["passed"] else 1


def mutate(manifest, **changes):
    manifest=copy.deepcopy(manifest);manifest["records"][0].update(changes);return manifest


def wrong_binding(manifest, algorithm=False):
    manifest=copy.deepcopy(manifest)
    manifest["records"][0]["binding"]["algorithm" if algorithm else "digest"] = "unsupported-test-v999" if algorithm else "incorrect-test-digest"
    return manifest


def future(manifest):
    manifest=copy.deepcopy(manifest);when=datetime.now(timezone.utc)+timedelta(days=1)
    manifest["records"][0].update(reviewed_at=stamp(when),expires_at=stamp(when+timedelta(days=1)))
    return manifest


if __name__=="__main__":
    try: sys.exit(main())
    except (OSError,ValueError,AssertionError,subprocess.TimeoutExpired) as error:
        print(f"design review consumer probe failed: {error}",file=sys.stderr);sys.exit(1)
