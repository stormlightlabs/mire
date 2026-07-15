package review

import (
	"fmt"
	"path"
	"sort"
	"strings"
)

func resolvePolicies(input Input, guidance []Guidance, files []FileChange) (PolicyResolution, []ContextArtifact, error) {
	rules := builtInRules()
	privateRules := append([]PolicyRule(nil), input.Request.Rules...)
	if len(privateRules) == 0 && input.Request.Configuration != "" {
		privateRules = ParsePolicyRules(input.Request.Configuration, PolicyTierPrivate, "private_request")
	}
	for index, rule := range privateRules {
		rule.Tier = PolicyTierPrivate
		if rule.Source == "" {
			rule.Source = "private_request"
		}
		if err := validateRule(rule); err != nil {
			return PolicyResolution{}, nil, err
		}
		privateRules[index] = rule
		rules = append(rules, rule)
	}
	artifacts := make([]ContextArtifact, 0)
	for _, item := range guidance {
		if item.Tier == PolicyTierBasePolicy {
			for _, rule := range item.Rules {
				rule.Tier = PolicyTierBasePolicy
				if rule.Source == "" {
					rule.Source = item.ID
				}
				if rule.Scope == "" {
					rule.Scope = item.Scope
				}
				if err := validateRule(rule); err != nil {
					return PolicyResolution{}, nil, err
				}
				rules = append(rules, rule)
			}
		}
		if item.Tier == PolicyTierTargetEvidence {
			artifacts = append(artifacts, ContextArtifact{ID: item.ID, Kind: string(item.Kind), Source: "target_snapshot_evidence", Path: item.Path, Tier: item.Tier, Content: item.Content, Digest: item.Digest})
			if input.NoBaseRevision && item.Kind == GuidanceTargetPolicy {
				for _, rule := range item.Rules {
					rule.Tier = PolicyTierTargetEvidence
					if rule.Source == "" {
						rule.Source = item.ID
					}
					if rule.Scope == "" {
						rule.Scope = item.Scope
					}
					if err := validateRule(rule); err != nil {
						return PolicyResolution{}, nil, err
					}
					rules = append(rules, rule)
				}
			}
		}
	}
	paths := make([]string, 0, len(files))
	for _, file := range files {
		name := file.TargetPath
		if name == "" {
			name = file.BasePath
		}
		paths = append(paths, name)
	}
	if len(paths) == 0 {
		paths = []string{""}
	}
	sort.Strings(paths)
	decisions := make([]PolicyDecision, 0)
	conflicts := make([]PolicyConflict, 0)
	for _, pathName := range paths {
		keys := map[string]bool{}
		for _, rule := range rules {
			if ruleApplies(rule, pathName) {
				keys[rule.Key] = true
			}
		}
		keyList := make([]string, 0, len(keys))
		for key := range keys {
			keyList = append(keyList, key)
		}
		sort.Strings(keyList)
		for _, key := range keyList {
			candidates := applicableRules(rules, key, pathName)
			selected, conflict := selectRule(candidates)
			decisions = append(decisions, PolicyDecision{Path: pathName, Key: key, Candidates: candidates, Selected: selected})
			if conflict != nil {
				conflict.Path = pathName
				conflicts = append(conflicts, *conflict)
			}
		}
	}
	resolution := PolicyResolution{Decisions: decisions, Conflicts: conflicts, TargetEvidence: artifacts, NoBaseRevisionException: input.NoBaseRevision}
	digest, err := digestValue(struct {
		Decisions []PolicyDecision
		Conflicts []PolicyConflict
		Target    []ContextArtifact
		Exception bool
	}{decisions, conflicts, artifacts, input.NoBaseRevision})
	if err != nil {
		return PolicyResolution{}, nil, err
	}
	resolution.Digest = digest
	return resolution, artifacts, nil
}

func builtInRules() []PolicyRule {
	return []PolicyRule{
		{Key: "repository_write", Value: "deny", Tier: PolicyTierBuiltIn, Source: "mire_builtin_safety"},
		{Key: "command_execution", Value: "deny", Tier: PolicyTierBuiltIn, Source: "mire_builtin_safety"},
		{Key: "network_access", Value: "deny", Tier: PolicyTierBuiltIn, Source: "mire_builtin_safety"},
		{Key: "model_tools", Value: "snapshot_read_only", Tier: PolicyTierBuiltIn, Source: "mire_builtin_safety"},
		{Key: "evidence_floor", Value: "snapshot_anchor_required", Tier: PolicyTierBuiltIn, Source: "mire_builtin_safety"},
	}
}

func validateRule(rule PolicyRule) error {
	if strings.TrimSpace(rule.Key) == "" || strings.TrimSpace(rule.Value) == "" {
		return fmt.Errorf("assemble review model: policy rule key and value are required")
	}
	if rule.Scope != "" {
		if strings.HasPrefix(rule.Scope, "/") || strings.Contains(rule.Scope, "..") {
			return fmt.Errorf("assemble review model: unsafe policy scope %q", rule.Scope)
		}
	}
	return nil
}

func ruleApplies(rule PolicyRule, filePath string) bool {
	if rule.Scope == "" {
		return true
	}
	if rule.Scope == filePath {
		return true
	}
	matched, err := pathMatch(rule.Scope, filePath)
	return err == nil && matched
}

func pathMatch(pattern, value string) (bool, error) {
	matched, err := path.Match(pattern, value)
	if err == nil && matched {
		return true, nil
	}
	// A directory scope applies to descendants without turning policy into a
	// filesystem query.
	return strings.HasPrefix(value, strings.TrimSuffix(pattern, "/")+"/"), err
}

func applicableRules(rules []PolicyRule, key, filePath string) []PolicyRule {
	result := make([]PolicyRule, 0)
	for _, rule := range rules {
		if rule.Key == key && ruleApplies(rule, filePath) {
			result = append(result, rule)
		}
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].Tier != result[j].Tier {
			return result[i].Tier < result[j].Tier
		}
		if specificity(result[i]) != specificity(result[j]) {
			return specificity(result[i]) > specificity(result[j])
		}
		if result[i].Source != result[j].Source {
			return result[i].Source < result[j].Source
		}
		return result[i].Value < result[j].Value
	})
	return result
}

func specificity(rule PolicyRule) int {
	if rule.Scope == "" {
		return 0
	}
	return len(rule.Scope) + 1
}

func selectRule(candidates []PolicyRule) (PolicyRule, *PolicyConflict) {
	if len(candidates) == 0 {
		return PolicyRule{}, nil
	}
	winnerTier, winnerSpecificity := candidates[0].Tier, specificity(candidates[0])
	winning := make([]PolicyRule, 0)
	for _, candidate := range candidates {
		if candidate.Tier == winnerTier && specificity(candidate) == winnerSpecificity {
			winning = append(winning, candidate)
		}
	}
	selected := winning[0]
	values := map[string]bool{selected.Value: true}
	for _, candidate := range winning[1:] {
		values[candidate.Value] = true
	}
	if len(values) == 1 {
		return selected, nil
	}
	selected.Value = saferValue(winning)
	conflict := &PolicyConflict{Key: selected.Key, Tier: winnerTier, Rules: winning, Selected: selected.Value}
	return selected, conflict
}

func saferValue(rules []PolicyRule) string {
	for _, rule := range rules {
		normalized := strings.ToLower(strings.TrimSpace(rule.Value))
		if normalized == "deny" || normalized == "false" || normalized == "none" || normalized == "disabled" || normalized == "forbid" {
			return rule.Value
		}
	}
	minimum, allNumeric := int64(0), true
	for index, rule := range rules {
		value, ok := parseInt(rule.Value)
		if !ok {
			allNumeric = false
			break
		}
		if index == 0 || value < minimum {
			minimum = value
		}
	}
	if allNumeric {
		return fmt.Sprintf("%d", minimum)
	}
	values := make([]string, 0, len(rules))
	for _, rule := range rules {
		values = append(values, rule.Value)
	}
	sort.Strings(values)
	return values[0]
}
