Build the Project
# Kernel Driver Project

This project is a kernel driver that provides functionality for [insert purpose here].

## Prerequisites

Before building and installing the driver, make sure you have the following:

- [Windows Driver Kit (WDK)](https://docs.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk)
- [Rust programming language](https://www.rust-lang.org/tools/install)

## Set up the Environment

To set up the environment variables for the WDK, follow these steps:

```sh
call "C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\setenv.bat" x64
```

## Build the Project

To build the project, run the following command:

```sh
cargo build --release
```

## Generate a signing certificate

To create a signing certificate for your application, you can use the following steps:

1. Open a command prompt and navigate to the directory where you want to generate the certificate.

2. Run the following command to generate a self-signed certificate:

```sh
makecert -r -pe -n "CN=Your Certificate Name" -ss My -sr LocalMachine -a sha256 -sky signature -cy end -sv MyCertificate.pvk MyCertificate.cer
```

    This command will generate a private key file (`MyCertificate.pvk`) and a certificate file (`MyCertificate.cer`).

3. Import the certificate into the certificate store by running the following command:

```sh
certutil -addstore My MyCertificate.cer
```

    This will import the certificate into the "Personal" certificate store.

4. Export the certificate with the private key by running the following command:

```sh
pvk2pfx -pvk MyCertificate.pvk -spc MyCertificate.cer -pfx MyCertificate.pfx
```

    This will generate a PFX file (`MyCertificate.pfx`) that contains both the private key and the certificate.

5. You can now use the generated certificate (`MyCertificate.pfx`) to sign your driver using the `signtool` utility as mentioned in the previous section.

Remember to keep the private key file (`MyCertificate.pvk`) and the PFX file (`MyCertificate.pfx`) secure.

For more information on certificate generation and management, refer to the [Microsoft documentation](https://docs.microsoft.com/en-us/windows/win32/seccrypto/makecert-usage).


## Sign the Driver

To install the driver on modern Windows systems, it must be signed. Follow these steps:

1. Obtain a code-signing certificate.
2. Use the signtool utility provided by the WDK to sign your driver:

```sh
signtool sign /v /s My /n "Your Certificate Name" /t http://timestamp.verisign.com/scripts/timestamp.dll path\to\your\driver.sys
```

## Install the Driver

To install the driver, follow these steps:

1. Use `sc` to create a new service for your driver:

```sh
sc create kernel_driver type= kernel binPath= path\to\your\driver.sys
```

2. Start the driver service:

```sh
sc start kernel_driver
```

For more information, refer to the [WDK documentation](https://docs.microsoft.com/en-us/windows-hardware/drivers/develop/).