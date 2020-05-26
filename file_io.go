package main

import (
	"fmt"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"time"

	yaml "gopkg.in/yaml.v2"
)

var pluralsightDir = ".pluralsight"
var credentialsFileName = "credentials.yaml"

func getPluralsightFile(fileName string) (*os.File, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("Error getting the user's home directory when trying to open file: %s\n%s", fileName, err)
	}

	filePath := filepath.Join(home, pluralsightDir, fileName)
	file, err := os.OpenFile(filePath, os.O_APPEND|os.O_CREATE|os.O_RDWR, 0644)

	if os.IsNotExist(err) {
		err = os.MkdirAll(filepath.Join(home, pluralsightDir), 0777)
		if err != nil {
			return nil, fmt.Errorf("Error making the .pluralsight dir when trying to create file: %s\n%s", fileName, err)
		}
		file, err = os.Create(filePath)
		if err != nil {
			return nil, fmt.Errorf("Error creating the file %s in the newly created .pluralsight dir\n%s", fileName, err)
		}
	} else if err != nil {
		return nil, fmt.Errorf("Error opening the file %s. Not a 'Is Not Exist' error\n%s", fileName, err)
	}

	return file, nil
}

func setLogFile(logFile string) *os.File {
	file, err := getPluralsightFile(logFile)
	panicOnError(err, "Could not get, create, or append to log file")

	log.SetOutput(file)
	return file
}

func fetchCredentials(fileName string) (credentials, error) {
	creds := credentials{}
	file, err := getPluralsightFile(fileName)

	if err != nil {
		return creds, err
	}

	defer file.Close()

	bytes, err := ioutil.ReadAll(file)
	if err != nil {
		return creds, fmt.Errorf("Error reading credentials file to bytes\n%s", err)
	}

	yaml.Unmarshal(bytes, &creds)

	return creds, nil
}

func getPayload() []byte {
	var payload []byte
	ch := make(chan []byte)

	go func() {
		stdin, err := ioutil.ReadAll(os.Stdin)
		panicOnError(err, "Error trying to read all from stdin")
		ch <- stdin
	}()

	select {
	case payload = <-ch:
	case <-time.After(10000 * time.Millisecond):
		log.Println("Timeout reading from stdin, exiting now")
		panic("")
	}

	return payload
}
