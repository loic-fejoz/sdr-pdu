# **Projet : PlutoSDR 2FSK Gateway (Rust Native)**

## **1\. Contexte & Objectif**

Développer un binaire Rust optimisé pour l'architecture **ARMv7-A (Cortex-A9)** du **PlutoSDR**.  
L'application doit interfacer des flux réseaux standards (KISS/CAT) avec l'émetteur RF **AD9361/AD9363** pour réaliser des transmissions satellites en **2FSK** (Uplink VHF).

## **2\. Architecture Logicielle & Concurrence**

L'application utilise le runtime tokio pour gérer l'asynchronisme :

### **A. Entrées Réseau (Multi-Producteurs)**

* **Serveur KISS (TCP) :** \* Écoute sur le port 8001\.  
  * Accepte **plusieurs clients simultanés**.  
  * Désencapsule le protocole KISS (Framing 0xC0).  
  * Envoie les trames décodées dans un canal tokio::sync::mpsc::channel.  
* **Serveur CAT (TCP/UDP) :** \* Compatible protocole rigctld (Hamlib).  
  * Commande principale : F \<fréquence\> (fixe la fréquence porteuse).  
  * Met à jour une valeur partagée (ex: AtomicU64) pour la gestion du Doppler.

### **B. Moteur d'Émission (Consommateur Unique)**

Une boucle de traitement dédiée surveille le canal mpsc. Pour chaque trame reçue :

1. Récupère la dernière fréquence Doppler.  
2. Calcule le buffer IQ complet (2FSK) en RAM.  
3. Configure le LO (Local Oscillator) du PlutoSDR via libad9361-iio.  
4. Pousse le buffer vers le DMA via libiio.

## **3\. Détails du DSP (NCO & Modulation)**

* **Modulation :** 2FSK à phase continue (CPFSK).  
* **NCO (Numerically Controlled Oscillator) :**  
  * **Référence :** S'inspirer de fxpt\_phase.rs de FutureSDR.  
  * **Logique :** Utiliser une phase en virgule fixe (u32) où $2^{32}$ représente $2\\pi$.  
  * **Conversion Sin/Cos :** Utiliser une table de recherche (LUT) pré-calculée.  
  * **Optimisation NEON (SIMD) :** \* Le code doit être compatible avec les instructions ARM NEON.  
    * Vectoriser l'accumulation de phase et l'accès à la LUT pour traiter 4 ou 8 échantillons par itération.  
    * Utiliser les intrinsèques core::arch::arm ou structurer les boucles pour l'auto-vectorisation du compilateur (rustc avec target-cpu=cortex-a9).  
* **Format de sortie :** i16 (Complex Signed 16-bit) entrelacé, prêt pour le DAC de l'AD9361.

## **4\. Spécifications Hardware (IIO & AD9361)**

* **Device IIO :** cf-ad9361-lpc pour le TX (DAC).  
* **Channels :** voltage0 (I) et voltage1 (Q).  
* **Local Oscillator (LO) :** Canal altvoltage1 du device ad9361-phy.  
* **Initialisation RF :**  
  * Sampling Rate : 1 MSPS.  
  * Bandwidth : 200 kHz.  
  * TX Attenuation : Configurable dynamiquement.

## **5\. Guide d'implémentation pour le LLM Code**

### **Exemple de structure NCO Vectorisée**

// Approche conceptuelle pour NEON  
fn modulate\_neon(data: &\[u8\], phase\_acc: \&mut u32, phase\_inc: u32) \-\> Vec\<i16\> {  
    // Utiliser des chunks de 4 ou 8 pour exploiter les registres Q (128-bit)  
    // d'ARM NEON. Chaque u32 de phase est accumulé en parallèle.  
}

### **Contraintes de compilation**

* **Target Triple :** arm-unknown-linux-gnueabihf.  
* **Features CPU :** \+v7, \+vfp3, \+neon.  
* **Linkage :** Liaison dynamique avec libiio.so et libad9361.so (présents sur le Pluto).

## **6\. Sécurité & Robustesse**

* **Silence RF :** S'assurer que l'émetteur ne génère pas de porteuse parasite entre les trames (Zero-fill ou désactivation des buffers).  
* **Gestion de Queue :** Les trames KISS sont traitées dans l'ordre d'arrivée (FIFO).  
* **Multi-clients :** Chaque handler tokio::spawn doit proprement fermer son canal en cas de déconnexion sans affecter les autres clients.