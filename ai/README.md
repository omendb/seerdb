# ai/ Directory - Development Documentation

**Purpose**: Internal development documentation, research notes, and planning documents
**Audience**: Developers working on seerdb
**Last Updated**: November 8, 2025

---

## Quick Start

**New to the project?** Read in this order:
1. `CURRENT_STATE.md` - TL;DR of where we are
2. `PRODUCTION_READINESS.md` - Roadmap to 0.0.1
3. `BUGS_AND_EDGE_CASES.md` - Known issues to fix
4. `DECISIONS.md` - Why we built things this way

---

## Active Documents (Current, Up-to-Date)

### Planning & Status
- **`CURRENT_STATE.md`** - Current status, priorities, immediate next steps
- **`PRODUCTION_READINESS.md`** - Comprehensive roadmap to 0.0.1 (8 weeks)
- **`BUGS_AND_EDGE_CASES.md`** - All known bugs (critical to minor)
- **`TODO.md`** - Active tasks

### Code & API Reviews
- **`API_REVIEW.md`** - Batch API critique and fixes needed
- **`OPTIMIZATION_STATUS.md`** - Performance status, what to defer

### Historical Record
- **`DECISIONS.md`** - Design decisions with rationale (keep updating)
- **`STATUS.md`** - Detailed performance history and milestones

---

## Archived/Old Documents (Superseded, Can Delete)

### Superseded by PRODUCTION_READINESS.md
- `PLAN.md` - Old project plan (outdated)
- `PHASE_2_PLAN.md` - Old phase plan (completed)
- `PHASE_2_COMPLETE.md` - Old milestone (completed)
- `OPTIMIZATION_PLAN.md` - Superseded by OPTIMIZATION_STATUS.md

### Superseded by BUGS_AND_EDGE_CASES.md
- `CRITICAL_BUGS.md` - Partial list (incomplete)
- `CRITICAL_LEAK_FINDINGS.md` - Specific issue (now in BUGS)
- `UNWRAP_AUDIT.md` - Specific audit (now in BUGS)

### Superseded by STATUS.md / CURRENT_STATE.md
- `BENCHMARKS.md` - Old benchmarks (outdated)
- `COMPETITIVE_ADVANTAGES.md` - Covered in CURRENT_STATE
- `CONTEXT.md` - Covered in CURRENT_STATE

### Specialized Docs (Keep for Reference)
- `ARCHITECTURE.md` - System architecture (still relevant)
- `RESEARCH.md` - Paper summaries (still relevant)
- `CHECKSUM_DESIGN.md` - Checksum implementation plan (TODO item)
- `KWAY_MERGE_PLAN.md` - K-way merge design (implemented, historical)
- `SOTA_EXPERIMENTS.md` - SOTA library experiments (historical)
- `SOTA_SESSION_SUMMARY.md` - SOTA session notes (historical)
- `VLOG_BENCHMARK.md` - VLog performance data (historical)

### Testing Docs (Active but Needs Cleanup)
- `CRASH_RECOVERY_TESTS.md` - Crash recovery test plan (TODO)
- `FUZZING.md` - Fuzzing strategy (TODO)
- `LEAK_DETECTION.md` - Memory leak detection (TODO)
- `PRACTICAL_SOAK_TESTS.md` - Soak testing plan (TODO)
- `SOAK_TESTING.md` - Soak testing strategy (TODO)
- `STRESS_TESTS.md` - Stress testing plan (TODO)

---

## Cleanup Recommendations

### Delete (Superseded)
```bash
rm ai/PLAN.md
rm ai/PHASE_2_PLAN.md
rm ai/PHASE_2_COMPLETE.md
rm ai/OPTIMIZATION_PLAN.md
rm ai/CRITICAL_BUGS.md
rm ai/CRITICAL_LEAK_FINDINGS.md
rm ai/UNWRAP_AUDIT.md
rm ai/BENCHMARKS.md
rm ai/COMPETITIVE_ADVANTAGES.md
rm ai/CONTEXT.md
```

### Archive (Historical Value)
```bash
mkdir -p ai/archive/
mv ai/KWAY_MERGE_PLAN.md ai/archive/
mv ai/SOTA_EXPERIMENTS.md ai/archive/
mv ai/SOTA_SESSION_SUMMARY.md ai/archive/
mv ai/VLOG_BENCHMARK.md ai/archive/
mv ai/CHECKSUM_DESIGN.md ai/archive/  # Will rewrite when implementing
```

### Consolidate Testing Docs
```bash
# Merge all testing docs into one comprehensive plan
# Keep: CRASH_RECOVERY_TESTS.md, FUZZING.md
# Delete duplicates: SOAK_TESTING.md, PRACTICAL_SOAK_TESTS.md
```

---

## Document Structure Standards

### File Naming
- `SCREAMING_SNAKE_CASE.md` for major docs
- Clear, descriptive names
- No dates in filenames (use "Last Updated" in content)

### Document Format
```markdown
# Title - Brief Description

**Date**: November 8, 2025
**Status**: Active/Archived/Superseded
**Purpose**: One-line description

---

## TL;DR

Quick summary in 2-3 bullets

---

## Content

...

---

**Last Updated**: November 8, 2025
**Next Review**: [When to review again]
```

### Required Sections
1. **TL;DR** - Quick summary
2. **Status** - Active/archived/superseded
3. **Last Updated** - Date of last edit
4. **Next Review** - When to review again

---

## Workflow

### When Starting Work
1. Read `CURRENT_STATE.md` for current status
2. Check `TODO.md` for active tasks
3. Review `BUGS_AND_EDGE_CASES.md` for known issues

### When Making Decisions
1. Document in `DECISIONS.md` with rationale
2. Update `CURRENT_STATE.md` if priorities change
3. Update `TODO.md` with action items

### When Completing Work
1. Update `STATUS.md` with results
2. Update `TODO.md` (mark complete, add follow-ups)
3. Update `CURRENT_STATE.md` if major milestone

### When Discovering Bugs
1. Add to `BUGS_AND_EDGE_CASES.md` with severity
2. Update `TODO.md` if actionable now
3. Update `PRODUCTION_READINESS.md` if affects timeline

---

## Key Principles

### Documentation
- **Current > Complete** - Keep docs current, remove outdated content
- **Actionable > Detailed** - Focus on what to do next
- **Concise > Comprehensive** - Be brief, link to details

### Maintenance
- Review every 2 weeks
- Delete superseded docs immediately
- Archive historical docs (don't delete)
- Update "Last Updated" on every edit

### Organization
- Active docs in root (ai/)
- Archives in ai/archive/
- Research in ai/research/
- Design in ai/design/

---

## Research Subdirectory

### ai/research/
- Paper summaries
- Benchmark results
- Experimental findings
- Competitive analysis

### ai/design/
- Design proposals
- Architecture diagrams
- API designs
- Format specifications

---

## Git Workflow

### What to Commit
- ✅ Active planning docs (CURRENT_STATE, TODO)
- ✅ Decision records (DECISIONS)
- ✅ Architecture docs (ARCHITECTURE)
- ✅ Major milestones (STATUS updates)

### What NOT to Commit
- ❌ Temporary notes
- ❌ Scratch work
- ❌ Personal todos
- ❌ Sensitive data

### Commit Messages
```
docs(ai): update CURRENT_STATE with Week 1 progress

- Mark block cache fix complete
- Update timeline
- Add test coverage metrics
```

---

## Tools & Scripts

### Cleanup Script
```bash
#!/bin/bash
# ai/cleanup.sh - Remove old/superseded docs

# Delete superseded files
rm -f ai/PLAN.md ai/PHASE_2_*.md ai/OPTIMIZATION_PLAN.md
rm -f ai/CRITICAL_BUGS.md ai/BENCHMARKS.md ai/CONTEXT.md

# Archive historical files
mkdir -p ai/archive/
mv ai/KWAY_MERGE_PLAN.md ai/SOTA_*.md ai/VLOG_BENCHMARK.md ai/archive/ 2>/dev/null

echo "Cleanup complete!"
```

### Doc Validation
```bash
#!/bin/bash
# ai/validate.sh - Check doc structure

for file in ai/*.md; do
    if ! grep -q "Last Updated" "$file"; then
        echo "Missing 'Last Updated': $file"
    fi
    if ! grep -q "TL;DR\|Executive Summary" "$file"; then
        echo "Missing summary: $file"
    fi
done
```

---

## FAQ

### Q: Which doc should I update when...

**...I find a bug?** → `BUGS_AND_EDGE_CASES.md`
**...I make a decision?** → `DECISIONS.md`
**...I complete a milestone?** → `STATUS.md` + `CURRENT_STATE.md`
**...I discover a performance issue?** → `OPTIMIZATION_STATUS.md`
**...I plan next week's work?** → `TODO.md`

### Q: How often should I update docs?

**Daily**: `TODO.md` (as tasks change)
**Weekly**: `CURRENT_STATE.md` (progress updates)
**Milestone**: `STATUS.md` (major achievements)
**As Needed**: `DECISIONS.md`, `BUGS_AND_EDGE_CASES.md`

### Q: What's the difference between...

**STATUS.md** vs **CURRENT_STATE.md**:
- STATUS: Detailed history, all milestones, comprehensive
- CURRENT_STATE: TL;DR, current focus, immediate priorities

**BUGS_AND_EDGE_CASES.md** vs **TODO.md**:
- BUGS: All known issues (catalog)
- TODO: Active tasks (work plan)

**PRODUCTION_READINESS.md** vs **CURRENT_STATE.md**:
- PRODUCTION_READINESS: 8-week roadmap (strategic)
- CURRENT_STATE: This week's focus (tactical)

---

**Last Updated**: November 8, 2025
**Maintainer**: Primary developers
**Next Cleanup**: November 15, 2025
