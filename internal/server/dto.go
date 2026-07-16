package server

import (
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/shared"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

type sessionDTO struct {
	ID             string `json:"id"`
	RepositoryID   string `json:"repository_id"`
	RepositoryName string `json:"repository_name"`
	Title          string `json:"title"`
	CreatedAt      string `json:"created_at"`
	CurrentRoundID string `json:"current_round_id,omitempty"`
}

type roundDTO struct {
	ID         string `json:"id"`
	SessionID  string `json:"session_id"`
	SnapshotID string `json:"snapshot_id,omitempty"`
	Number     int    `json:"number"`
	Status     string `json:"status"`
	Revision   string `json:"revision"`
	CreatedAt  string `json:"created_at"`
	UpdatedAt  string `json:"updated_at"`
}

type operationDTO struct {
	ID         string `json:"id"`
	SessionID  string `json:"session_id"`
	RoundID    string `json:"round_id,omitempty"`
	Kind       string `json:"kind"`
	Status     string `json:"status"`
	Failure    string `json:"failure,omitempty"`
	Revision   string `json:"revision"`
	CreatedAt  string `json:"created_at"`
	UpdatedAt  string `json:"updated_at"`
	StartedAt  string `json:"started_at,omitempty"`
	FinishedAt string `json:"finished_at,omitempty"`
}

func mapSession(session *db.Session) *sessionDTO {
	if session == nil {
		return nil
	}
	return &sessionDTO{
		session.ID,
		session.RepositoryID,
		session.RepositoryName,
		session.Title,
		session.CreatedAt.UTC().Format(time.RFC3339Nano),
		session.CurrentRoundID,
	}
}

func mapSessions(sessions []db.Session) []*sessionDTO {
	result := make([]*sessionDTO, 0, len(sessions))
	for i := range sessions {
		result = append(result, mapSession(&sessions[i]))
	}
	return result
}

func makeRoundDTO(round db.Round) roundDTO {
	return roundDTO{
		round.ID,
		round.SessionID,
		round.SnapshotID,
		round.Number,
		string(round.Status),
		shared.Revision(round.UpdatedAt),
		round.CreatedAt.UTC().Format(time.RFC3339Nano),
		round.UpdatedAt.UTC().Format(time.RFC3339Nano),
	}
}

func mapRounds(rounds []db.Round) []roundDTO {
	result := make([]roundDTO, 0, len(rounds))
	for _, round := range rounds {
		result = append(result, makeRoundDTO(round))
	}
	return result
}

func makeOperationDTO(operation db.Operation) operationDTO {
	return operationDTO{
		operation.ID, operation.SessionID, operation.RoundID,
		string(operation.Kind), string(operation.Status),
		operation.Failure,
		shared.Revision(operation.UpdatedAt), operation.CreatedAt.UTC().Format(time.RFC3339Nano),
		operation.UpdatedAt.UTC().Format(time.RFC3339Nano),
		shared.OptionalTime(operation.StartedAt), shared.OptionalTime(operation.FinishedAt),
	}
}

func mapOperations(operations []db.Operation) []operationDTO {
	result := make([]operationDTO, 0, len(operations))
	for _, operation := range operations {
		result = append(result, makeOperationDTO(operation))
	}
	return result
}

func makeDivergenceDTO(report snapshot.DivergenceReport) map[string]any {
	return map[string]any{
		"snapshot_id":    report.SnapshotID,
		"status":         report.Status,
		"affected_paths": report.AffectedPaths,
		"affected_refs":  report.AffectedRefs,
		"message":        report.Message,
	}
}
