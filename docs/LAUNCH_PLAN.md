# NoString Launch Plan

*Bitcoin inheritance without trusted third parties.*

---

## Product Summary

NoString is a desktop app for Bitcoin inheritance planning that:
- Creates miniscript-based inheritance policies
- Uses timelocks (not custodians) for heir access
- Supports multi-heir cascades and Shamir backups (SLIP-39, Codex32)
- Requires periodic "check-ins" to reset timelocks
- Works air-gapped via Electrum-compatible PSBTs

**Target Users:** Bitcoin self-custody holders who want inheritance planning without trusting a company.

---

## Phase Assessment

**Current state:** Phase 1 (Internal) → Phase 2 (Alpha) ready

| Requirement | Status |
|-------------|--------|
| Core functionality | ✅ 115 tests passing |
| Production-ready code | ✅ No shortcuts/mocks |
| Desktop app | ✅ Tauri UI working |
| Documentation | ✅ README, guides, ops runbook |
| Testnet validation | 🔄 In progress |

---

## Launch Phases

### Phase 1: Internal (Current)
**Timeline:** Now - 1 week

- [x] Complete testnet validation
- [ ] Fund testnet wallet (waiting on faucet)
- [ ] End-to-end flow test (seed → heir → check-in → PSBT)
- [ ] Fix any bugs found in testing
- [ ] Code review with security focus

### Phase 2: Alpha Launch
**Timeline:** Week 2

**Actions:**
- [ ] Landing page at `nostring.dev` or similar
- [ ] Email capture for early access
- [ ] GitHub repo public (already have README)
- [ ] Announce in Bitcoin-specific channels:
  - Nostr (your @kiwihodl account)
  - Bitcoin Twitter
  - Stacker News

**Messaging:** "Open-source Bitcoin inheritance. No custodians. No monthly fees. Just math."

### Phase 3: Beta Launch
**Timeline:** Week 3-4

**Actions:**
- [ ] First 10-20 beta testers (hand-picked)
- [ ] Invite Bitcoin Butlers network
- [ ] Collect feedback on UX gaps
- [ ] Start content marketing:
  - "Why inheritance is the next self-custody problem"
  - "How timelocks replace trusted third parties"

**Consider:**
- Beta badge in app
- Feedback widget built-in
- Optional telemetry (opt-in only, privacy-preserving)

### Phase 4: Early Access
**Timeline:** Month 2

**Actions:**
- [ ] Open waitlist in batches (50-100 users)
- [ ] Gather quantitative data (completion rates, drop-off points)
- [ ] Run user interviews (incentivize with sats via Lightning)
- [ ] Security audit prep (document threat model, scope)

**Expansion strategy:** Throttled invites (10% batches) to manage support load

### Phase 5: Full Launch
**Timeline:** Month 3+

**Actions:**
- [ ] Open self-serve downloads
- [ ] Product Hunt launch (Bitcoin/crypto category)
- [ ] Hacker News Show HN post
- [ ] Press outreach to Bitcoin publications:
  - Bitcoin Magazine
  - Citadel Dispatch
  - Bitcoin Audible

---

## ORB Channel Strategy

### Owned (Build First)
| Channel | Action | Priority |
|---------|--------|----------|
| Email list | Landing page capture | ✅ High |
| Blog | nostring.dev/blog | Medium |
| Nostr | Long-form posts | ✅ High |
| GitHub | README + Discussions | ✅ High |

### Rented (Amplify)
| Channel | Approach |
|---------|----------|
| Twitter/X | Threads on inheritance problem, link to landing |
| Stacker News | Post announcements, engage in comments |
| Reddit r/bitcoin | Share when ready, don't spam |

### Borrowed (Accelerate)
| Opportunity | Target |
|-------------|--------|
| Podcast interviews | Citadel Dispatch, What Bitcoin Did, Stephan Livera |
| Newsletter features | Bitcoin Optech, Marty's Bent |
| Influencer demos | Send to Bitcoin security-focused creators |

---

## Launch Assets Needed

### Landing Page
- [ ] Domain (nostring.dev / nostring.app / getnostring.com)
- [ ] Clear value prop above fold
- [ ] How it works (3-step visual)
- [ ] Email capture form
- [ ] "Built by Bitcoin Butlers" badge (optional)

### Visual Assets
- [ ] Logo / icon
- [ ] Screenshots of app
- [ ] Demo video (2-3 min)
- [ ] Architecture diagram

### Content
- [ ] Launch blog post
- [ ] Twitter thread script
- [ ] Nostr announcement
- [ ] Product Hunt description + tagline

---

## Product Hunt Strategy

**When:** After 100+ beta users, polished UX, social proof

**Preparation:**
1. Build hunter relationships (find Bitcoin-friendly hunters)
2. Optimize listing:
   - Tagline: "Bitcoin inheritance without trusted third parties"
   - Demo video showing full flow
   - Screenshots of key screens
3. Prep launch day engagement team

**Launch day:**
- All-day monitoring
- Respond to every comment
- Drive traffic to email signup

---

## Success Metrics

| Phase | Metric | Target |
|-------|--------|--------|
| Alpha | Email signups | 100+ |
| Beta | Active testers | 20+ |
| Early Access | Policies created | 50+ |
| Full Launch | Downloads | 500+ |
| 6 months | Active users | 1000+ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Security vulnerability | Pre-launch audit, bug bounty program |
| Low adoption | Focus on education, explain the problem first |
| UX too complex | Wizard-style onboarding, progressive disclosure |
| Competition | Move fast, open-source moat, community ownership |

---

## Next Actions

1. **Today:** Complete testnet validation
2. **This week:** 
   - Register domain
   - Create landing page repo
   - Draft Twitter thread
3. **Next week:**
   - Deploy landing page
   - Start alpha announcements

---

*Last updated: 2026-02-02*
