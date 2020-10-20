const exec = require('@actions/exec');
const core = require('@actions/core');
const child_process = require('child_process');
const fs = require("fs");

async function shell_cmd(cmd) {
    return await new Promise((resolve, reject) => {
        child_process.exec(cmd, function(error, stdout, stderr) {
            if (error) {
                console.log(`... error=${error}`)
                reject(error)
            }

            if (stderr) {
                console.log(`... stderr=${stderr.trim()}`)
            }

            console.log(`... stdout=${stdout.trim()}`)
            resolve(stdout.trim());
        });
    });
}

(async () => {
    try {
        console.log(`Start main.js`)

        var dev_suffix = (core.getInput('github_event_name') == "release") ? "" : "-dev";
        const versioned_label = `v${fs.readFileSync('./version.txt').toString().trim()}${dev_suffix}`;
        const latest_label = `latest${dev_suffix}`;
        console.log(`Use labels: versioned=${versioned_label} latest=${latest_label}`);

        console.log(`Login into Dockerhub user=${core.getInput('dockerhub_username')}`);
        await shell_cmd(`echo "${core.getInput('dockerhub_password')}" | docker login -u ${core.getInput('dockerhub_username')} --password-stdin`);

        console.log(`Login into Container Registry user=${core.getInput('container_registry_username')} repo=${core.getInput('container_registry_base_url')}`);
        await shell_cmd(`echo "${core.getInput('container_registry_password')}" | docker login -u ${core.getInput('container_registry_username')} --password-stdin ${core.getInput('container_registry_base_url')}`);

        process.env.DOCKER_CLI_EXPERIMENTAL = `enabled`
        process.env.PREFIX = `${core.getInput('container_prefix')}`
        process.env.LABEL_PREFIX = `${versioned_label}`

        console.log(`echo Create multi-arch versioned manifest`)
        await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-multi-arch-create`)

        console.log(`echo Inspect multi-arch versioned manifest`)
        await exec.exec(`docker manifest inspect ${core.getInput('container_prefix')}/${core.getInput('container_name')}:${versioned_label}`)

        console.log(`echo Push multi-arch versioned manifest`)
        await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-multi-arch-push`)

        process.env.LABEL_PREFIX = `${latest_label}`

        console.log(`echo Create multi-arch latest manifest`)
        await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-multi-arch-create`)

        console.log(`echo Inspect multi-arch latest manifest`)
        await exec.exec(`docker manifest inspect ${core.getInput('container_prefix')}/${core.getInput('container_name')}:${latest_label}`)

        console.log(`echo Push multi-arch latest manifest`)
        await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-multi-arch-push`)
    } catch (error) {
        core.setFailed(error);
    }        
})();