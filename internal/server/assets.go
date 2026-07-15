package server

import (
	"embed"
	"io/fs"
)

//go:embed static/**
var staticFiles embed.FS

func embeddedStaticFiles() fs.FS {
	files, err := fs.Sub(staticFiles, "static")
	if err != nil {
		panic(err)
	}
	return files
}
