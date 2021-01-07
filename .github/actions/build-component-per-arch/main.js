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

        console.log(`Use multiarch/qemu-user-static to configure cross-plat`);
        await shell_cmd('docker run --rm --privileged multiarch/qemu-user-static --reset -p yes');

        var dev_suffix = (core.getInput('github_event_name') == "release") ? "" : "-dev";
        const versioned_label = `v${fs.readFileSync('./version.txt').toString().trim()}${dev_suffix}`;
        const latest_label = `latest${dev_suffix}`;
        console.log(`Use labels: versioned=${versioned_label} latest=${latest_label}`);

        var push_containers = 0;
        if (core.getInput('github_event_name') == 'release') push_containers = 1;
        else if (core.getInput('github_event_name') == 'push' && 
                core.getInput('github_ref') == 'refs/heads/main') push_containers = 1;
        else if (core.getInput('github_event_name').startsWith('pull_request') && 
                core.getInput('github_event_action') == 'closed' && 
                core.getInput('github_ref') == 'refs/heads/main' && 
                core.getInput('github_merged') == 'true') push_containers = 1;
        else console.log(`Not pushing containers ... event: ${core.getInput('github_event_name')}, ref: ${core.getInput('github_ref')}, action: ${core.getInput('github_event_action')}, merged: ${core.getInput('github_merged')}`);
        console.log(`Push containers: ${push_containers}`);

        var makefile_target_suffix = "";
        switch (core.getInput('platform')) {
            case "amd64":   
                process.env.BUILD_AMD64 = 1
                makefile_target_suffix = "amd64"; 
                break;
            case "arm32v7": 
                process.env.BUILD_ARM32 = 1
                makefile_target_suffix = "arm32"; 
                break;
            case "arm64v8": 
                process.env.BUILD_ARM64 = 1
                makefile_target_suffix = "arm64"; 
                break;
            default:
                core.setFailed(`Failed with unknown platform: ${core.getInput('platform')}`)
                return
        }
        console.log(`Makefile build target suffix: ${makefile_target_suffix}`)

        if (core.getInput('build_rust') == '1') {
            console.log(`Install Rust`)
            child_process.execSync(`curl https://sh.rustup.rs | sh -s -- -y --default-toolchain=1.49.0`);
            const bindir = `${process.env.HOME}/.cargo/bin`;
            process.env.PATH = `${process.env.PATH}:${bindir}`;

            console.log(`Check cargo version`)
            await shell_cmd('cargo --version')
            console.log(`Install Cross`)
            await shell_cmd('make install-cross')
            await shell_cmd('cross --version')
            console.log(`Cross compile: akri-cross-build-${makefile_target_suffix}`)
            await exec.exec(`make akri-cross-build-${makefile_target_suffix}`)
        } else {
            console.log(`Not building Rust: ${core.getInput('build_rust')}`)
        }

        process.env.PREFIX = `${core.getInput('container_prefix')}`

        console.log(`Build the versioned container: make ${core.getInput('makefile_component_name')}-build-${makefile_target_suffix}`)
        process.env.LABEL_PREFIX = `${versioned_label}`
        await exec.exec(`make ${core.getInput('makefile_component_name')}-build-${makefile_target_suffix}`)

        console.log(`Build the latest container: make ${core.getInput('makefile_component_name')}-build-${makefile_target_suffix}`)
        process.env.LABEL_PREFIX = `${latest_label}`
        await exec.exec(`make ${core.getInput('makefile_component_name')}-build-${makefile_target_suffix}`)

        const image_name = `${core.getInput('container_prefix')}/${core.getInput('container_name')}:${versioned_label}-${core.getInput('platform')}`
        console.log(`Check that container contains container-images-legal-notice.md: ${image_name}`)
        await shell_cmd(`docker run ${image_name} find container-images-legal-notice.md | wc -l | grep -v 0`)

        if (push_containers == "1") {
            console.log(`Login into Container Registry user=${core.getInput('container_registry_username')} repo=${core.getInput('container_registry_base_url')}`);
            await shell_cmd(`echo "${core.getInput('container_registry_password')}" | docker login -u ${core.getInput('container_registry_username')} --password-stdin ${core.getInput('container_registry_base_url')}`);
    
            console.log(`Push the versioned container: make ${core.getInput('makefile_component_name')}-docker-per-arch-${makefile_target_suffix}`)
            process.env.LABEL_PREFIX = `${versioned_label}`
            await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-per-arch-${makefile_target_suffix}`)

            console.log(`Push the latest container: make ${core.getInput('makefile_component_name')}-docker-per-arch-${makefile_target_suffix}`)
            process.env.LABEL_PREFIX = `${latest_label}`
            await exec.exec(`make ${core.getInput('makefile_component_name')}-docker-per-arch-${makefile_target_suffix}`)
        } else {
            console.log(`Not pushing containers: ${push_containers}`)
        }
    } catch (error) {
        core.setFailed(error);
    }        
})();